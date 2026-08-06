#include "passstore.h"

#include "i18n.h"
#include "process.h"

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>

namespace {

QProcessEnvironment storeEnvironment(const QString &root) {
    QProcessEnvironment env = QProcessEnvironment::systemEnvironment();
    env.insert(QStringLiteral("PASSWORD_STORE_DIR"), root);
    return env;
}

// Single entry point for every `pass` invocation, so failures are reported
// the same way everywhere.
bool runPass(const QString &root, const QStringList &args, const QByteArray &stdinData,
             QString *error) {
    const ProcResult result = runProcess(QStringLiteral("pass"), args, stdinData,
                                         storeEnvironment(root));
    if (!result.started) {
        if (error)
            *error = I18n::t(QStringLiteral("pass.spawn_error"), QStringLiteral("err"), result.err);
        return false;
    }
    if (!result.success) {
        if (error)
            *error = result.err.trimmed();
        return false;
    }
    return true;
}

void walk(const QString &root, const QString &dir, QStringList *entries, QStringList *groups) {
    QDirIterator it(dir, QDir::Dirs | QDir::Files | QDir::NoDotAndDotDot);
    while (it.hasNext()) {
        it.next();
        const QFileInfo info = it.fileInfo();
        const QString name = info.fileName();

        // Skips .git, .gpg-id, .extensions and every other hidden entry.
        if (name.startsWith(QLatin1Char('.')))
            continue;

        const QString relative = QDir(root).relativeFilePath(info.absoluteFilePath());
        if (info.isDir()) {
            groups->append(relative);
            walk(root, info.absoluteFilePath(), entries, groups);
        } else if (name.endsWith(QStringLiteral(".gpg"))) {
            entries->append(relative.chopped(4));
        }
    }
}

// Clears the gpg-agent cache so the passphrase is genuinely checked instead
// of being waved through by a cached one. RELOADAGENT hits the whole agent,
// so other tools sharing it lose their cached keys at this moment too.
void clearAgentCache() {
    runProcess(QStringLiteral("gpg-connect-agent"),
               {QStringLiteral("RELOADAGENT"), QStringLiteral("/bye")});
}

}

QVector<QPair<QString, QString>> PassStore::listSecretKeys() {
    QVector<QPair<QString, QString>> keys;

    const ProcResult result = runProcess(QStringLiteral("gpg"),
                                         {QStringLiteral("--list-secret-keys"),
                                          QStringLiteral("--with-colons")});
    if (!result.started)
        return keys;

    QString currentKeyId;
    const auto lines = result.out.split(QLatin1Char('\n'));
    for (const QString &line : lines) {
        const QStringList fields = line.split(QLatin1Char(':'));
        if (fields.isEmpty())
            continue;

        if (fields.first() == QStringLiteral("sec")) {
            currentKeyId = fields.value(4);
        } else if (fields.first() == QStringLiteral("uid") && !currentKeyId.isEmpty()) {
            keys.append({currentKeyId, fields.value(9)});
            currentKeyId.clear();
        }
    }

    return keys;
}

bool PassStore::isStore(const QString &root) {
    return QFile::exists(root + QStringLiteral("/.gpg-id"));
}

bool PassStore::initStore(const QString &root, const QString &gpgId, QString *error) {
    if (isStore(root)) {
        if (error)
            *error = I18n::t(QStringLiteral("pass.store_already_exists"), QStringLiteral("path"), root);
        return false;
    }

    if (!QDir().mkpath(root)) {
        if (error)
            *error = I18n::t(QStringLiteral("pass.mkdir_error"), QStringLiteral("err"), root);
        return false;
    }

    // `pass init` writes the .gpg-id and re-encrypts whatever is already
    // there — harmless on a store we just created empty.
    return runPass(root, {QStringLiteral("init"), gpgId}, QByteArray(), error);
}

void PassStore::list(QStringList *entries, QStringList *groups) const {
    walk(m_root, m_root, entries, groups);
    entries->sort();
    groups->sort();
}

bool PassStore::insert(const QString &entry, const PassEntryData &data, QString *error) const {
    return runPass(m_root,
                   {QStringLiteral("insert"), QStringLiteral("-m"), QStringLiteral("-f"), entry},
                   buildPassEntryContent(data), error);
}

bool PassStore::move(const QString &from, const QString &to, QString *error) const {
    return runPass(m_root, {QStringLiteral("mv"), QStringLiteral("-f"), from, to}, QByteArray(), error);
}

bool PassStore::remove(const QString &entry, QString *error) const {
    return runPass(m_root, {QStringLiteral("rm"), QStringLiteral("-f"), entry}, QByteArray(), error);
}

// Uses `pass rm -r` rather than removing the directory ourselves: it clears
// any leftover hidden file (a nested .gpg-id, for instance) along with the
// tree. Only called for groups the caller already found to be empty of
// visible entries and subgroups.
bool PassStore::removeGroup(const QString &group, QString *error) const {
    return runPass(m_root,
                   {QStringLiteral("rm"), QStringLiteral("-r"), QStringLiteral("-f"), group},
                   QByteArray(), error);
}

// Decrypts through gpg directly with `--pinentry-mode loopback`, taking the
// passphrase on stdin instead of relying on an external pinentry — omapass
// holds the passphrase for the session and unlocks entries with it.
bool PassStore::show(const QString &entry, const Secret &passphrase, PassEntryData *data,
                     QString *error) const {
    const QString filePath = m_root + QLatin1Char('/') + entry + QStringLiteral(".gpg");
    if (!QFile::exists(filePath)) {
        if (error)
            *error = I18n::t(QStringLiteral("pass.gpg_file_not_found"), QStringLiteral("entry"), entry);
        return false;
    }

    QByteArray stdinData = passphrase.bytes();
    stdinData.append('\n');

    const ProcResult result = runProcess(QStringLiteral("gpg"),
                                         {QStringLiteral("--batch"), QStringLiteral("--no-tty"),
                                          QStringLiteral("--pinentry-mode"), QStringLiteral("loopback"),
                                          QStringLiteral("--passphrase-fd"), QStringLiteral("0"),
                                          QStringLiteral("-d"), filePath},
                                         stdinData);
    stdinData.fill('\0');

    if (!result.started) {
        if (error)
            *error = I18n::t(QStringLiteral("pass.gpg_spawn_error"), QStringLiteral("err"), result.err);
        return false;
    }

    if (result.success) {
        *data = parsePassEntry(result.out);
        return true;
    }

    if (error) {
        const QString err = result.err.trimmed();
        if (err.contains(QStringLiteral("loopback")) || err.contains(QStringLiteral("pinentry"))
                || err.contains(QStringLiteral("Inappropriate ioctl")))
            *error = I18n::t(QStringLiteral("pass.gpg_loopback_not_configured"));
        else if (err.contains(QStringLiteral("bad passphrase"))
                 || err.contains(QStringLiteral("decryption failed")))
            *error = I18n::t(QStringLiteral("pass.wrong_gpg_password"));
        else
            *error = err;
    }
    return false;
}

// Checks the passphrase by decrypting the store's first entry. A store with
// no entries yet has nothing to check against, so the passphrase is accepted.
bool PassStore::verifyPassphrase(const Secret &passphrase, QString *error) const {
    clearAgentCache();

    QStringList entries;
    QStringList groups;
    list(&entries, &groups);
    if (entries.isEmpty())
        return true;

    PassEntryData data;
    return show(entries.first(), passphrase, &data, error);
}

PassEntryData parsePassEntry(const QString &content) {
    PassEntryData data;

    const QStringList lines = content.split(QLatin1Char('\n'));
    if (lines.isEmpty())
        return data;

    data.password = Secret(lines.first());

    for (int i = 1; i < lines.size(); ++i) {
        const QString &line = lines.at(i);
        const QString lower = line.toLower();

        const auto valueAfter = [&line](int prefixLength) {
            return line.mid(prefixLength).trimmed();
        };

        if (lower.startsWith(QStringLiteral("username:")))
            data.username = valueAfter(9);
        else if (lower.startsWith(QStringLiteral("user:")))
            data.username = valueAfter(5);
        else if (lower.startsWith(QStringLiteral("login:")))
            data.username = valueAfter(6);
        else if (lower.startsWith(QStringLiteral("url:")))
            data.url = valueAfter(4);
        else if (!line.trimmed().isEmpty())
            data.extra.append(line);
    }

    return data;
}

QByteArray buildPassEntryContent(const PassEntryData &data) {
    QByteArray content = data.password.bytes();
    content.append('\n');

    if (!data.username.isEmpty())
        content.append(QStringLiteral("username: %1\n").arg(data.username).toUtf8());
    if (!data.url.isEmpty())
        content.append(QStringLiteral("url: %1\n").arg(data.url).toUtf8());
    for (const QString &line : data.extra)
        content.append(line.toUtf8()).append('\n');

    return content;
}
