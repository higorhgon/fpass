#include "appcontroller.h"

#include "clipboard.h"
#include "filter.h"
#include "i18n.h"
#include "passstore.h"

#include <QDir>
#include <QFileInfo>
#include <QRect>
#include <QSettings>

namespace {

const auto windowGeometrySetting = QStringLiteral("window/geometry");

QVariantMap describe(const DbRef &ref) {
    return {{QStringLiteral("path"), ref.path},
            {QStringLiteral("name"), QFileInfo(ref.path).fileName()},
            {QStringLiteral("kind"), Vault::kindLabel(ref.kind)}};
}

}

AppController::AppController(const AppConfig &config, QObject *parent)
    : QObject(parent), m_config(config), m_history(config.recencyEnabled) {
    m_messageTimer.setInterval(1000);
    connect(&m_messageTimer, &QTimer::timeout, this, [this]() {
        if (m_clipboardCountdown > 0) {
            m_clipboardCountdown -= 1;
            if (m_clipboardCountdown > 0) {
                emit messageChanged();
                return;
            }
        }
        clearMessage();
    });

    refreshDatabases();

    // With exactly one database there is nothing to choose: go straight to
    // its unlock prompt.
    if (m_databases.size() == 1)
        selectDatabase(0);
}

QVariantList AppController::databases() const {
    QVariantList list;
    list.reserve(m_filteredDatabases.size());
    for (const DbRef &ref : m_filteredDatabases)
        list.append(describe(ref));
    return list;
}

void AppController::setDatabaseQuery(const QString &query) {
    if (m_databaseQuery == query)
        return;

    m_databaseQuery = query;

    QStringList paths;
    for (const DbRef &ref : std::as_const(m_databases))
        paths.append(ref.path);
    const QStringList kept = filterItems(paths, query);

    m_filteredDatabases.clear();
    for (const DbRef &ref : std::as_const(m_databases)) {
        if (kept.contains(ref.path))
            m_filteredDatabases.append(ref);
    }

    emit databasesChanged();
}

QVariantMap AppController::pendingDatabase() const {
    if (!m_hasPendingDatabase)
        return {};
    return describe(m_pendingDatabase);
}

QString AppController::vaultLabel() const {
    if (m_vault.isNull())
        return QString();
    return QFileInfo(m_vault->path()).fileName();
}

bool AppController::passBackend() const {
    return !m_vault.isNull() && m_vault->kind() == VaultKind::Pass;
}

void AppController::refreshDatabases() {
    m_databases = Vault::findDatabases(m_config.searchPath);

    // Most-used first, so the database you actually open is the one under the
    // cursor when the window appears.
    QStringList paths;
    for (const DbRef &ref : std::as_const(m_databases))
        paths.append(ref.path);
    m_history.sortItems(paths);

    QVector<DbRef> sorted;
    sorted.reserve(m_databases.size());
    for (const QString &path : std::as_const(paths)) {
        for (const DbRef &ref : std::as_const(m_databases)) {
            if (ref.path == path) {
                sorted.append(ref);
                break;
            }
        }
    }
    m_databases = sorted;

    m_filteredDatabases = m_databases;
    m_databaseQuery.clear();
    emit databasesChanged();
}

void AppController::selectDatabase(int index) {
    if (index < 0 || index >= m_filteredDatabases.size())
        return;

    m_pendingDatabase = m_filteredDatabases.at(index);
    m_hasPendingDatabase = true;
    setUnlockError(QString());
    emit pendingDatabaseChanged();
}

void AppController::cancelUnlock() {
    m_hasPendingDatabase = false;
    setUnlockError(QString());
    emit pendingDatabaseChanged();
}

void AppController::unlock(const QString &password) {
    if (m_busy || !m_hasPendingDatabase)
        return;

    setBusy(true);
    setUnlockError(QString());

    // Deferred by one turn of the event loop so the busy state is painted
    // before the backend blocks on keepassxc-cli or gpg.
    const Secret secret(password);
    const DbRef ref = m_pendingDatabase;
    QTimer::singleShot(0, this, [this, ref, secret]() {
        openVault(ref, secret);
        setBusy(false);
    });
}

void AppController::openVault(const DbRef &ref, const Secret &secret) {
    QString error;
    Vault *vault = Vault::open(ref, secret, &error);
    if (!vault) {
        setUnlockError(error);
        return;
    }

    m_vault.reset(vault);
    m_history.recordUse(ref.path);

    m_hasPendingDatabase = false;
    emit pendingDatabaseChanged();

    m_query.clear();
    refreshEntries();

    m_stage = QStringLiteral("entries");
    emit stageChanged();
}

void AppController::refreshEntries() {
    m_allEntries.clear();
    m_groups.clear();
    if (!m_vault.isNull())
        m_vault->list(&m_allEntries, &m_groups);

    m_history.sortItems(m_allEntries);
    applyEntryFilter();
}

void AppController::applyEntryFilter() {
    m_filteredEntries = filterItems(m_allEntries, m_query);
    emit entriesChanged();
}

void AppController::setQuery(const QString &query) {
    if (m_query == query)
        return;

    m_query = query;
    applyEntryFilter();
}

bool AppController::isEmptyGroup(const QString &entry) const {
    return entry.endsWith(Vault::emptyGroupSuffix());
}

QString AppController::groupNameOf(const QString &entry) const {
    const QString suffix = Vault::emptyGroupSuffix();
    return entry.endsWith(suffix) ? entry.chopped(suffix.size()) : entry;
}

void AppController::copyPassword(const QString &entry) {
    if (m_vault.isNull())
        return;

    if (isEmptyGroup(entry)) {
        showMessage(I18n::t(QStringLiteral("app.empty_group")), true);
        return;
    }

    m_history.recordUse(entry);

    Secret password;
    QString error;
    if (!m_vault->fetchPassword(entry, &password, &error)) {
        showMessage(I18n::t(QStringLiteral("app.copy_password_error")), true);
        return;
    }

    if (!Clipboard::copy(password.toString())) {
        showMessage(I18n::t(QStringLiteral("app.copy_password_error")), true);
        return;
    }

    showClipboardMessage(I18n::t(QStringLiteral("app.copied_entry"), QStringLiteral("entry"), entry));
    Clipboard::scheduleClear(password);
}

void AppController::copyText(const QString &text) {
    if (text.isEmpty())
        return;

    if (Clipboard::copy(text))
        showMessage(I18n::t(QStringLiteral("app.copied_to_clipboard")), false);
    else
        showMessage(I18n::t(QStringLiteral("app.copy_error")), true);
}

QVariantMap AppController::entryDetails(const QString &entry) {
    if (m_vault.isNull() || isEmptyGroup(entry)) {
        showMessage(I18n::t(QStringLiteral("app.empty_group")), true);
        return {};
    }

    const QString fallback = entry.section(QLatin1Char('/'), -1);

    EntryData data;
    QString error;
    m_vault->fetchEntry(entry, &data, &error);

    return {{QStringLiteral("title"), m_vault->titleFor(entry, fallback)},
            {QStringLiteral("url"), data.url},
            {QStringLiteral("notes"), data.notes}};
}

// The password comes back in the clear because an editable field has to hold
// it; it is the one place fpass cannot keep a secret inside `Secret` alone.
QVariantMap AppController::entryFields(const QString &entry) {
    QVariantMap fields;
    fields.insert(QStringLiteral("originalPath"), entry);

    const int slash = entry.lastIndexOf(QLatin1Char('/'));
    fields.insert(QStringLiteral("group"), slash < 0 ? QString() : entry.left(slash));
    fields.insert(QStringLiteral("title"), slash < 0 ? entry : entry.mid(slash + 1));

    if (m_vault.isNull())
        return fields;

    EntryData data;
    QString error;
    if (!m_vault->fetchEntry(entry, &data, &error)) {
        showMessage(I18n::t(QStringLiteral("app.read_entry_error"), QStringLiteral("err"), error), true);
        return fields;
    }

    fields.insert(QStringLiteral("username"), data.username);
    fields.insert(QStringLiteral("password"), data.password.toString());
    fields.insert(QStringLiteral("url"), data.url);
    fields.insert(QStringLiteral("notes"), data.notes);
    fields.insert(QStringLiteral("extra"), data.extra);
    return fields;
}

void AppController::saveEntry(const QVariantMap &fields) {
    if (m_vault.isNull())
        return;

    QString group = fields.value(QStringLiteral("group")).toString().trimmed();
    while (group.endsWith(QLatin1Char('/')))
        group.chop(1);

    const QString title = fields.value(QStringLiteral("title")).toString().trimmed();
    if (title.isEmpty()) {
        showMessage(I18n::t(QStringLiteral("app.title_empty")), true);
        return;
    }

    const QString path = group.isEmpty() ? title : group + QLatin1Char('/') + title;
    const bool isEdit = fields.value(QStringLiteral("isEdit")).toBool();

    EntryData data;
    data.username = fields.value(QStringLiteral("username")).toString().trimmed();
    data.password = Secret(fields.value(QStringLiteral("password")).toString());
    data.url = fields.value(QStringLiteral("url")).toString().trimmed();
    data.notes = fields.value(QStringLiteral("notes")).toString();
    data.extra = fields.value(QStringLiteral("extra")).toStringList();

    QString error;
    const bool ok = isEdit
        ? m_vault->editEntry(fields.value(QStringLiteral("originalPath")).toString(), path, group,
                             data, &error)
        : m_vault->addEntry(path, group, data, &error);

    if (ok) {
        if (!isEdit)
            m_history.recordUse(path);
        showMessage(I18n::t(isEdit ? QStringLiteral("app.entry_edited")
                                   : QStringLiteral("app.entry_added")),
                    false);
    } else {
        showMessage(I18n::t(isEdit ? QStringLiteral("app.edit_error") : QStringLiteral("app.add_error")),
                    true);
    }

    refreshEntries();
}

void AppController::deleteEntry(const QString &entry) {
    if (m_vault.isNull())
        return;

    const bool emptyGroup = isEmptyGroup(entry);
    QString error;
    const bool ok = emptyGroup ? m_vault->removeGroup(groupNameOf(entry), &error)
                               : m_vault->removeEntry(entry, &error);

    if (ok) {
        showMessage(I18n::t(emptyGroup ? QStringLiteral("app.group_deleted")
                                       : QStringLiteral("app.entry_deleted")),
                    false);
        refreshEntries();
    } else {
        showMessage(I18n::t(QStringLiteral("app.delete_error"), QStringLiteral("err"), error), true);
    }
}

void AppController::renameGroup(const QString &entry, const QString &newName) {
    if (m_vault.isNull())
        return;

    const QString trimmed = newName.trimmed();
    if (trimmed.isEmpty()) {
        showMessage(I18n::t(QStringLiteral("app.name_empty")), true);
        return;
    }

    QString error;
    if (m_vault->renameGroup(groupNameOf(entry), trimmed, &error))
        showMessage(I18n::t(QStringLiteral("app.group_renamed")), false);
    else
        showMessage(I18n::t(QStringLiteral("app.rename_group_error"), QStringLiteral("err"), error), true);

    refreshEntries();
}

QStringList AppController::matchingGroups(const QString &prefix) const {
    return filterItems(m_groups, prefix);
}

void AppController::createKeepassDatabase(const QString &name, const QString &password) {
    if (m_busy || name.trimmed().isEmpty())
        return;

    setBusy(true);
    const Secret secret(password);
    const QString trimmedName = name.trimmed();
    QTimer::singleShot(0, this, [this, trimmedName, secret]() {
        QString path;
        QString error;
        if (!Vault::createKeepassDatabase(trimmedName, secret, &path, &error)) {
            setUnlockError(error);
            setBusy(false);
            return;
        }

        refreshDatabases();
        emit databaseCreated();
        openVault({path, VaultKind::Keepass}, secret);
        setBusy(false);
    });
}

QVariantList AppController::gpgKeys() const {
    QVariantList keys;
    const auto secretKeys = PassStore::listSecretKeys();
    for (const auto &key : secretKeys) {
        keys.append(QVariantMap{{QStringLiteral("id"), key.first},
                                {QStringLiteral("identity"), key.second}});
    }
    return keys;
}

QString AppController::defaultPassStoreDirectory() const {
    return QDir::homePath() + QStringLiteral("/.password-store");
}

// Completions for the directory field: the subdirectories matching the
// segment being typed (typing "/home/user/.pas" suggests folders under
// "/home/user/" starting with "pas"), hidden ones excluded.
QStringList AppController::directorySuggestions(const QString &path) const {
    const int slash = path.lastIndexOf(QLatin1Char('/'));
    const QString parent = slash < 0 ? QString() : path.left(slash + 1);
    const QString partial = slash < 0 ? path : path.mid(slash + 1);

    const QDir dir(parent.isEmpty() ? QStringLiteral(".") : parent);
    QStringList suggestions;
    const auto names = dir.entryList(QDir::Dirs | QDir::NoDotAndDotDot, QDir::Name);
    for (const QString &name : names) {
        if (name.startsWith(QLatin1Char('.')))
            continue;
        if (name.startsWith(partial, Qt::CaseInsensitive))
            suggestions.append(parent + name);
    }
    return suggestions;
}

void AppController::createPassStore(const QString &directory, const QString &keyId) {
    if (m_busy)
        return;

    const QString dir = directory.trimmed();
    if (dir.isEmpty()) {
        setUnlockError(I18n::t(QStringLiteral("db_app.dir_empty")));
        return;
    }
    if (keyId.isEmpty()) {
        setUnlockError(I18n::t(QStringLiteral("db_app.no_gpg_key_selected")));
        return;
    }

    setBusy(true);
    QTimer::singleShot(0, this, [this, dir, keyId]() {
        QString error;
        if (!PassStore::initStore(dir, keyId, &error)) {
            setUnlockError(error);
            setBusy(false);
            return;
        }

        refreshDatabases();
        emit databaseCreated();

        // A brand-new store still needs its GPG passphrase, so it goes
        // through the same unlock prompt as any other database.
        m_pendingDatabase = {dir, VaultKind::Pass};
        m_hasPendingDatabase = true;
        setUnlockError(QString());
        emit pendingDatabaseChanged();
        setBusy(false);
    });
}

QString AppController::message() const {
    if (m_messageText.isEmpty())
        return QString();

    if (m_clipboardCountdown < 0)
        return m_messageText;

    return I18n::t(QStringLiteral("ui.clipboard_clear_countdown"),
                   {{QStringLiteral("msg"), m_messageText},
                    {QStringLiteral("secs"), m_clipboardCountdown}});
}

void AppController::showMessage(const QString &text, bool isError) {
    m_messageText = text;
    m_messageIsError = isError;
    m_clipboardCountdown = -1;
    emit messageChanged();

    m_messageTimer.stop();
    QTimer::singleShot(3000, this, [this, text]() {
        // Only clears the message it was scheduled for; a newer one owns the
        // status line from the moment it is set.
        if (m_messageText == text && m_clipboardCountdown < 0)
            clearMessage();
    });
}

void AppController::showClipboardMessage(const QString &text) {
    m_messageText = text;
    m_messageIsError = false;
    m_clipboardCountdown = Clipboard::clearSecs;
    emit messageChanged();
    m_messageTimer.start();
}

void AppController::clearMessage() {
    m_messageTimer.stop();
    if (m_messageText.isEmpty())
        return;

    m_messageText.clear();
    m_clipboardCountdown = -1;
    emit messageChanged();
}

void AppController::setBusy(bool busy) {
    if (m_busy == busy)
        return;

    m_busy = busy;
    emit busyChanged();
}

void AppController::setUnlockError(const QString &error) {
    if (m_unlockError == error)
        return;

    m_unlockError = error;
    emit unlockErrorChanged();
}

QVariantMap AppController::windowGeometry() const {
    QSettings settings;
    const QRect geometry = settings.value(windowGeometrySetting).toRect();

    // Positions are legitimately negative on monitors left of or above the
    // primary one, so validity travels separately instead of being encoded
    // as -1.
    return {{QStringLiteral("valid"), geometry.isValid()},
            {QStringLiteral("x"), geometry.x()},
            {QStringLiteral("y"), geometry.y()},
            {QStringLiteral("width"), geometry.width()},
            {QStringLiteral("height"), geometry.height()},
            {QStringLiteral("maximized"),
             settings.value(QStringLiteral("window/maximized"), false).toBool()}};
}

void AppController::saveWindowGeometry(int x, int y, int width, int height, bool maximized) {
    QSettings settings;
    settings.setValue(windowGeometrySetting, QRect(x, y, width, height));
    settings.setValue(QStringLiteral("window/maximized"), maximized);
}
