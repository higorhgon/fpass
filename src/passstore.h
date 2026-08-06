#pragma once

#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

#include "secret.h"

// Backend for `pass`, the standard unix password manager.
//
// Security notes:
// - Listing reads the filesystem directly (the .gpg filenames); nothing is
//   decrypted to build the list.
// - Entry secrets travel in `Secret`, which wipes itself on destruction.

// The conventional pass entry layout: line 1 is the password, the lines after
// it are "key: value" metadata. Lines we do not model are kept verbatim in
// `extra` so another tool's data (an OTP seed, say) survives an edit here.
struct PassEntryData {
    Secret password;
    QString username;
    QString url;
    QStringList extra;
};

class PassStore {
public:
    // GPG secret keys in the local keyring as (keyid, identity) pairs, for
    // choosing who a new store encrypts to. A key with several UIDs shows up
    // once, under its primary identity.
    static QVector<QPair<QString, QString>> listSecretKeys();

    static bool isStore(const QString &root);
    static bool initStore(const QString &root, const QString &gpgId, QString *error);

    explicit PassStore(const QString &root) : m_root(root) {}

    QString root() const { return m_root; }

    void list(QStringList *entries, QStringList *groups) const;

    bool insert(const QString &entry, const PassEntryData &data, QString *error) const;
    bool move(const QString &from, const QString &to, QString *error) const;
    bool remove(const QString &entry, QString *error) const;
    bool removeGroup(const QString &group, QString *error) const;

    bool show(const QString &entry, const Secret &passphrase, PassEntryData *data,
              QString *error) const;
    bool verifyPassphrase(const Secret &passphrase, QString *error) const;

private:
    QString m_root;
};

// Exposed for testing.
PassEntryData parsePassEntry(const QString &content);
QByteArray buildPassEntryContent(const PassEntryData &data);
