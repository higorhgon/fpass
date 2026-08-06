#pragma once

#include <QHash>
#include <QString>
#include <QStringList>

#include "secret.h"

// Frecency engine: remembers how often and how recently each item was used,
// to rank the list by relevance.
//
// Entry paths are never written in the clear. The file keys off an
// HMAC-SHA256 taken with a random local key (created once, kept in
// ~/.config/fpass/.history_key at 0600) rather than a plain digest, so
// guessed paths cannot be confirmed against the file with a rainbow table.
class History {
public:
    explicit History(bool enabled);

    void recordUse(const QString &item);
    quint64 score(const QString &item) const;
    void sortItems(QStringList &items) const;

private:
    QString hashItem(const QString &item) const;
    void save() const;

    struct Record {
        quint32 count = 0;
        quint64 timestamp = 0;
    };

    QHash<QString, Record> m_records;
    QString m_filePath;
    bool m_enabled;
    Secret m_key;
};

// Frecency weight for how long ago (in seconds) an item was last used. Free
// function so the scoring curve can be tested on its own.
quint64 weightForAge(quint64 ageSecs);
