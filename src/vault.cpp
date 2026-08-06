#include "vault.h"

#include "config.h"
#include "i18n.h"
#include "passstore.h"
#include "process.h"

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QRegularExpression>

namespace {

QString keepassDatabaseDir() {
    return Config::configDir() + QStringLiteral("/databases");
}

// Finds files matching `pattern` under `path`. Prefers `fd`, which is what
// fpass has always used and is markedly faster over a whole home directory;
// falls back to walking the tree when it is not installed.
QStringList findFiles(const QString &pattern, const QString &path, bool includeHidden) {
    QStringList results;

    QStringList args;
    if (includeHidden)
        args << QStringLiteral("--hidden") << QStringLiteral("--no-ignore");
    args << pattern << path;

    const ProcResult fd = runProcess(QStringLiteral("fd"), args);
    if (fd.started) {
        const auto lines = fd.out.split(QLatin1Char('\n'));
        for (const QString &line : lines) {
            const QString trimmed = line.trimmed();
            if (!trimmed.isEmpty())
                results.append(trimmed);
        }
        return results;
    }

    QDir::Filters filters = QDir::Files | QDir::NoDotAndDotDot;
    if (includeHidden)
        filters |= QDir::Hidden;

    QDirIterator it(path, filters, QDirIterator::Subdirectories);
    while (it.hasNext()) {
        it.next();
        if (QRegularExpression(pattern).match(it.fileName()).hasMatch())
            results.append(it.filePath());
    }
    return results;
}

// Roots of pass stores: the parent directory of every .gpg-id found. A
// narrower-scope .gpg-id inside a store already found is not a database of
// its own — only the outermost root is kept.
QStringList findPassStores(const QString &searchPath) {
    QStringList roots;
    const QStringList markers = findFiles(QStringLiteral("^\\.gpg-id$"), searchPath, true);
    for (const QString &marker : markers) {
        const QString parent = QFileInfo(marker).absolutePath();
        if (!roots.contains(parent))
            roots.append(parent);
    }
    roots.sort();

    const QStringList all = roots;
    QStringList outermost;
    for (const QString &root : all) {
        const bool nested = std::any_of(all.cbegin(), all.cend(), [&root](const QString &other) {
            return other != root && root.startsWith(other + QLatin1Char('/'));
        });
        if (!nested)
            outermost.append(root);
    }
    return outermost;
}

QString parentGroup(const QString &group) {
    const int slash = group.lastIndexOf(QLatin1Char('/'));
    return slash < 0 ? QString() : group.left(slash);
}

PassEntryData toPassData(const EntryData &data) {
    PassEntryData pd;
    pd.password = data.password;
    pd.username = data.username;
    pd.url = data.url;
    pd.extra = data.extra;
    return pd;
}

// Tags every group with no entries or subgroups below it, so it still shows
// up in the list.
void appendEmptyGroups(const QStringList &groups, QStringList *entries) {
    for (const QString &group : groups) {
        const QString prefix = group + QLatin1Char('/');
        const bool hasChildren =
            std::any_of(entries->cbegin(), entries->cend(),
                        [&prefix](const QString &e) { return e.startsWith(prefix); })
            || std::any_of(groups.cbegin(), groups.cend(),
                           [&prefix](const QString &g) { return g.startsWith(prefix); });
        if (!hasChildren)
            entries->append(group + Vault::emptyGroupSuffix());
    }
}

}

KpResult runKpcli(const QStringList &args, const QVector<Secret> &stdinLines) {
    QByteArray stdinData;
    for (const Secret &line : stdinLines)
        stdinData.append(line.bytes()).append('\n');

    const ProcResult result = runProcess(QStringLiteral("keepassxc-cli"), args, stdinData);
    stdinData.fill('\0');

    KpResult kp;
    kp.started = result.started;
    kp.success = result.success;
    kp.out = result.out;
    kp.err = result.started
        ? result.err
        : I18n::t(QStringLiteral("keepass.spawn_error"), QStringLiteral("err"), result.err);
    return kp;
}

QString Vault::emptyGroupSuffix() {
    return QLatin1Char('/') + I18n::t(QStringLiteral("common.empty_group_marker"));
}

QString Vault::kindLabel(VaultKind kind) {
    return kind == VaultKind::Keepass ? QStringLiteral("KeePassXC") : QStringLiteral("pass");
}

QVector<DbRef> Vault::findDatabases(const QString &searchPath) {
    QVector<DbRef> databases;

    QStringList kdbx = findFiles(QStringLiteral(".kdbx$"), searchPath, false);
    kdbx += findFiles(QStringLiteral(".kdbx$"), keepassDatabaseDir(), false);
    kdbx.sort();
    kdbx.removeDuplicates();
    for (const QString &path : kdbx)
        databases.append({path, VaultKind::Keepass});

    QStringList stores = findPassStores(searchPath);

    // The conventional location is always considered, even when `path` was
    // customised to somewhere that does not contain it.
    const QString defaultStore = QDir::homePath() + QStringLiteral("/.password-store");
    if (PassStore::isStore(defaultStore) && !stores.contains(defaultStore))
        stores.append(defaultStore);

    for (const QString &root : stores)
        databases.append({root, VaultKind::Pass});

    return databases;
}

bool Vault::createKeepassDatabase(const QString &name, const Secret &password,
                                  QString *createdPath, QString *error) {
    const QString dir = keepassDatabaseDir();
    if (!QDir().mkpath(dir)) {
        *error = I18n::t(QStringLiteral("keepass.mkdir_error"), QStringLiteral("err"), dir);
        return false;
    }

    const QString path = dir + QLatin1Char('/') + name + QStringLiteral(".kdbx");
    if (QFile::exists(path)) {
        *error = I18n::t(QStringLiteral("keepass.file_exists"));
        return false;
    }

    const KpResult result = runKpcli({QStringLiteral("db-create"), QStringLiteral("-p"), path},
                                     {password, password});
    if (!result.success) {
        *error = result.err;
        return false;
    }

    *createdPath = path;
    return true;
}

Vault *Vault::open(const DbRef &ref, const Secret &secret, QString *error) {
    if (ref.kind == VaultKind::Keepass) {
        const KpResult result = runKpcli({QStringLiteral("ls"), QStringLiteral("-q"), ref.path},
                                         {secret});
        if (!result.started) {
            *error = result.err;
            return nullptr;
        }
        if (!result.success) {
            *error = I18n::t(QStringLiteral("backend.wrong_password"));
            return nullptr;
        }
        return new Vault(VaultKind::Keepass, ref.path, secret);
    }

    if (!PassStore::isStore(ref.path)) {
        *error = I18n::t(QStringLiteral("pass.store_not_initialized"), QStringLiteral("path"), ref.path);
        return nullptr;
    }

    const PassStore store(ref.path);
    if (!store.verifyPassphrase(secret, error))
        return nullptr;

    return new Vault(VaultKind::Pass, ref.path, secret);
}

void Vault::list(QStringList *entries, QStringList *groups) const {
    entries->clear();
    groups->clear();

    if (m_kind == VaultKind::Keepass) {
        const KpResult result = runKpcli({QStringLiteral("ls"), QStringLiteral("-Rfq"), m_path},
                                         {m_secret});
        if (!result.success)
            return;

        QStringList lines;
        const auto rawLines = result.out.split(QLatin1Char('\n'));
        for (const QString &raw : rawLines) {
            const QString line = raw.trimmed();
            if (line.isEmpty())
                continue;
            lines.append(line);
            if (line.endsWith(QLatin1Char('/')))
                groups->append(line.chopped(1));
            else
                entries->append(line);
        }

        // `ls -Rfq` lists a group both as "Work/" and through its children,
        // so emptiness is decided against the raw listing rather than the
        // split lists.
        for (const QString &group : std::as_const(*groups)) {
            const QString prefix = group + QLatin1Char('/');
            const bool isEmpty = !std::any_of(lines.cbegin(), lines.cend(),
                                              [&prefix](const QString &line) {
                return line.startsWith(prefix) && line != prefix;
            });
            if (isEmpty)
                entries->append(group + emptyGroupSuffix());
        }
        return;
    }

    PassStore(m_path).list(entries, groups);
    appendEmptyGroups(*groups, entries);
}

QString Vault::titleFor(const QString &entryPath, const QString &fallback) const {
    if (m_kind != VaultKind::Keepass)
        return fallback;

    const KpResult result = runKpcli({QStringLiteral("show"), QStringLiteral("-q"), m_path, entryPath,
                                      QStringLiteral("-a"), QStringLiteral("Title")},
                                     {m_secret});
    const QString title = result.success ? result.out.trimmed() : QString();
    return title.isEmpty() ? fallback : title;
}

bool Vault::fetchPassword(const QString &entryPath, Secret *password, QString *error) const {
    if (m_kind == VaultKind::Keepass) {
        const KpResult result = runKpcli({QStringLiteral("show"), QStringLiteral("-q"), m_path,
                                          entryPath, QStringLiteral("-a"), QStringLiteral("Password")},
                                         {m_secret});
        if (!result.success) {
            *error = result.err;
            return false;
        }
        *password = Secret(result.out.trimmed());
        return true;
    }

    PassEntryData data;
    if (!PassStore(m_path).show(entryPath, m_secret, &data, error))
        return false;

    *password = data.password;
    return true;
}

bool Vault::fetchEntry(const QString &entryPath, EntryData *data, QString *error) const {
    if (m_kind == VaultKind::Keepass) {
        const auto attribute = [this, &entryPath](const QString &name) {
            const KpResult result = runKpcli({QStringLiteral("show"), QStringLiteral("-q"), m_path,
                                              entryPath, QStringLiteral("-a"), name},
                                             {m_secret});
            return result.success ? result.out.trimmed() : QString();
        };

        data->username = attribute(QStringLiteral("UserName"));
        data->password = Secret(attribute(QStringLiteral("Password")));
        data->url = attribute(QStringLiteral("URL"));
        data->notes = attribute(QStringLiteral("Notes"));
        data->extra.clear();
        return true;
    }

    PassEntryData pd;
    if (!PassStore(m_path).show(entryPath, m_secret, &pd, error))
        return false;

    data->username = pd.username;
    data->password = pd.password;
    data->url = pd.url;
    data->notes = pd.extra.join(QLatin1Char('\n'));
    data->extra = pd.extra;
    return true;
}

bool Vault::addEntry(const QString &entryPath, const QString &group, const EntryData &data,
                     QString *error) const {
    if (m_kind == VaultKind::Keepass) {
        if (!group.isEmpty())
            runKpcli({QStringLiteral("mkdir"), QStringLiteral("-q"), m_path, group}, {m_secret});

        const KpResult result = runKpcli({QStringLiteral("add"), QStringLiteral("-q"),
                                          QStringLiteral("-p"), QStringLiteral("-u"), data.username,
                                          QStringLiteral("--url"), data.url,
                                          QStringLiteral("--notes"), data.notes, m_path, entryPath},
                                         {m_secret, data.password, data.password});
        if (!result.success) {
            *error = result.err;
            return false;
        }
        return true;
    }

    return PassStore(m_path).insert(entryPath, toPassData(data), error);
}

bool Vault::editEntry(const QString &oldPath, const QString &newPath, const QString &group,
                      const EntryData &data, QString *error) const {
    if (m_kind == VaultKind::Keepass) {
        if (!group.isEmpty())
            runKpcli({QStringLiteral("mkdir"), QStringLiteral("-q"), m_path, group}, {m_secret});

        if (newPath != oldPath) {
            // keepassxc-cli's `mv` takes [database] [source] [target group];
            // the root is spelled "/".
            const QString destination = group.isEmpty() ? QStringLiteral("/") : group;
            runKpcli({QStringLiteral("mv"), QStringLiteral("-q"), m_path, oldPath, destination},
                     {m_secret});
        }

        const KpResult result = runKpcli({QStringLiteral("edit"), QStringLiteral("-q"),
                                          QStringLiteral("-p"), QStringLiteral("-u"), data.username,
                                          QStringLiteral("--url"), data.url,
                                          QStringLiteral("--notes"), data.notes, m_path, newPath},
                                         {m_secret, data.password, data.password});
        if (!result.success) {
            *error = result.err;
            return false;
        }
        return true;
    }

    const PassStore store(m_path);
    if (!store.insert(oldPath, toPassData(data), error))
        return false;
    if (newPath != oldPath)
        return store.move(oldPath, newPath, error);
    return true;
}

bool Vault::removeEntry(const QString &entryPath, QString *error) const {
    if (m_kind == VaultKind::Keepass) {
        const KpResult result = runKpcli({QStringLiteral("rm"), QStringLiteral("-q"), m_path, entryPath},
                                         {m_secret});
        if (!result.success) {
            *error = result.err;
            return false;
        }
        return true;
    }

    return PassStore(m_path).remove(entryPath, error);
}

bool Vault::renameGroup(const QString &oldGroup, const QString &newName, QString *error) const {
    const QString parent = parentGroup(oldGroup);
    const QString newGroup = parent.isEmpty() ? newName : parent + QLatin1Char('/') + newName;

    if (m_kind == VaultKind::Pass)
        return PassStore(m_path).move(oldGroup, newGroup, error);

    // keepassxc-cli has no native group rename: rebuild the subtree under the
    // new name, move each entry across keeping its relative path, then delete
    // the old (now empty) tree from the leaves up.
    QStringList entries;
    QStringList groups;
    list(&entries, &groups);
    const QString oldPrefix = oldGroup + QLatin1Char('/');

    // Every step is checked and aborts on the first failure: this is
    // multi-step (a sequence of mkdir/mv), so swallowing an error midway
    // would leave the group tree half-rebuilt.
    const auto mkdir = [this, error](const QString &path) {
        const KpResult result = runKpcli({QStringLiteral("mkdir"), QStringLiteral("-q"), m_path, path},
                                         {m_secret});
        if (!result.success) {
            *error = result.err;
            return false;
        }
        return true;
    };
    const auto move = [this, error](const QString &source, const QString &destinationGroup) {
        const KpResult result = runKpcli({QStringLiteral("mv"), QStringLiteral("-q"), m_path, source,
                                          destinationGroup},
                                         {m_secret});
        if (!result.success) {
            *error = result.err;
            return false;
        }
        return true;
    };

    // The new group is always created, even when `oldGroup` has no children —
    // otherwise an empty group would simply vanish, since the mkdir/mv calls
    // below only build destinations for content that actually exists.
    if (!mkdir(newGroup))
        return false;

    for (const QString &group : std::as_const(groups)) {
        if (!group.startsWith(oldPrefix))
            continue;
        if (!mkdir(newGroup + QLatin1Char('/') + group.mid(oldPrefix.size())))
            return false;
    }

    const QString suffix = emptyGroupSuffix();
    for (const QString &entry : std::as_const(entries)) {
        if (entry.endsWith(suffix) || !entry.startsWith(oldPrefix))
            continue;

        const QString relative = entry.mid(oldPrefix.size());
        const int slash = relative.lastIndexOf(QLatin1Char('/'));
        const QString destination = slash < 0
            ? newGroup
            : newGroup + QLatin1Char('/') + relative.left(slash);

        if (!mkdir(destination) || !move(entry, destination))
            return false;
    }

    QStringList oldTree;
    for (const QString &group : std::as_const(groups)) {
        if (group == oldGroup || group.startsWith(oldPrefix))
            oldTree.append(group);
    }
    std::sort(oldTree.begin(), oldTree.end(), [](const QString &a, const QString &b) {
        return a.size() > b.size();
    });
    for (const QString &group : std::as_const(oldTree)) {
        const KpResult result = runKpcli({QStringLiteral("rmdir"), QStringLiteral("-q"), m_path, group},
                                         {m_secret});
        if (!result.success) {
            *error = result.err;
            return false;
        }
    }

    return true;
}

bool Vault::removeGroup(const QString &group, QString *error) const {
    if (m_kind == VaultKind::Keepass) {
        const KpResult result = runKpcli({QStringLiteral("rmdir"), QStringLiteral("-q"), m_path, group},
                                         {m_secret});
        if (!result.success) {
            *error = result.err;
            return false;
        }
        return true;
    }

    return PassStore(m_path).removeGroup(group, error);
}
