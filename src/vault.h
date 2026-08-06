#pragma once

#include <QString>
#include <QStringList>
#include <QVector>

#include "secret.h"

// Abstraction over the two supported password backends: KeePassXC (through
// `keepassxc-cli`) and pass (through gpg). The rest of the app only ever
// talks to Vault/DbRef, so neither backend's command shapes leak into the
// interface.

enum class VaultKind { Keepass, Pass };

// A database found by the search, before it is opened.
struct DbRef {
    QString path;
    VaultKind kind = VaultKind::Keepass;
};

// An entry's fields, as the union of what the two formats carry. `extra` is
// only used by the pass backend, to preserve lines it does not model (an OTP
// seed, say) when an entry that already had them is edited.
struct EntryData {
    QString username;
    Secret password;
    QString url;
    QString notes;
    QStringList extra;
};

// Result of one `keepassxc-cli` run.
struct KpResult {
    bool started = false;
    bool success = false;
    QString out;
    QString err;
};

// Runs `keepassxc-cli` with the given arguments, writing each item of
// `stdinLines` (database password, new password, confirmation…) as one line
// on stdin.
KpResult runKpcli(const QStringList &args, const QVector<Secret> &stdinLines);

class Vault {
public:
    // Suffix marking an empty group in the entry list (`Work/[empty]`), so it
    // stays navigable — and deletable — even with nothing inside. Translated,
    // but always produced and matched through this one function (the locale
    // cannot change mid-run), so display and detection never drift apart.
    static QString emptyGroupSuffix();
    static QString kindLabel(VaultKind kind);

    // Finds KeePassXC databases (.kdbx) and pass stores (any directory with a
    // .gpg-id) under `searchPath` — the same configurable directory for both,
    // so a store anywhere below it is found, not just ~/.password-store.
    static QVector<DbRef> findDatabases(const QString &searchPath);

    static bool createKeepassDatabase(const QString &name, const Secret &password,
                                      QString *createdPath, QString *error);

    // Opens and authenticates against `ref`, validating the password or
    // passphrase before handing back a usable Vault.
    static Vault *open(const DbRef &ref, const Secret &secret, QString *error);

    VaultKind kind() const { return m_kind; }
    QString path() const { return m_path; }

    // Lists entries and groups, tagging empty groups with the suffix above.
    void list(QStringList *entries, QStringList *groups) const;

    // An entry's display title. Under KeePassXC this can differ from the last
    // path segment (Title is its own attribute); under pass the title is
    // always the filename.
    QString titleFor(const QString &entryPath, const QString &fallback) const;

    // Fetches only the password, so copying does not pay for the other
    // attributes.
    bool fetchPassword(const QString &entryPath, Secret *password, QString *error) const;
    bool fetchEntry(const QString &entryPath, EntryData *data, QString *error) const;

    bool addEntry(const QString &entryPath, const QString &group, const EntryData &data,
                  QString *error) const;
    // Edits the entry at `oldPath`, moving it to `newPath` when the path
    // changed (group and/or title).
    bool editEntry(const QString &oldPath, const QString &newPath, const QString &group,
                   const EntryData &data, QString *error) const;
    bool removeEntry(const QString &entryPath, QString *error) const;

    // Renames the last segment of `oldGroup` to `newName`, keeping the parent
    // group and everything below it intact.
    bool renameGroup(const QString &oldGroup, const QString &newName, QString *error) const;
    bool removeGroup(const QString &group, QString *error) const;

private:
    Vault(VaultKind kind, const QString &path, const Secret &secret)
        : m_kind(kind), m_path(path), m_secret(secret) {}

    VaultKind m_kind;
    QString m_path;
    Secret m_secret;
};
