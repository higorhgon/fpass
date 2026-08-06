#include "history.h"

#include "config.h"

#include <QDateTime>
#include <QFile>
#include <QMessageAuthenticationCode>
#include <QRandomGenerator>
#include <QTextStream>

namespace {

const int keyLength = 32;

Secret loadOrCreateKey(const QString &path) {
    QFile file(path);
    if (file.open(QIODevice::ReadOnly)) {
        const QByteArray existing = file.readAll();
        if (existing.size() == keyLength)
            return Secret(existing);
        file.close();
    }

    QByteArray key(keyLength, Qt::Uninitialized);
    QRandomGenerator::system()->generate(reinterpret_cast<quint32 *>(key.data()),
                                         reinterpret_cast<quint32 *>(key.data() + keyLength));

    if (file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner);
        file.write(key);
        file.close();
    }

    Secret secret(key);
    key.fill('\0');
    return secret;
}

quint64 nowSecs() {
    return static_cast<quint64>(QDateTime::currentSecsSinceEpoch());
}

}

quint64 weightForAge(quint64 ageSecs) {
    if (ageSecs < 86400)
        return 100;
    if (ageSecs < 604800)
        return 50;
    if (ageSecs < 2592000)
        return 20;
    return 5;
}

History::History(bool enabled)
    : m_filePath(Config::configDir() + QStringLiteral("/history")), m_enabled(enabled) {
    m_key = loadOrCreateKey(Config::configDir() + QStringLiteral("/.history_key"));

    if (!m_enabled)
        return;

    QFile file(m_filePath);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
        return;

    QTextStream in(&file);
    while (!in.atEnd()) {
        const QStringList parts = in.readLine().split(QLatin1Char('|'));
        if (parts.size() != 3)
            continue;

        bool countOk = false;
        bool timeOk = false;
        const quint32 count = parts.at(1).toUInt(&countOk);
        const quint64 timestamp = parts.at(2).toULongLong(&timeOk);
        if (countOk && timeOk)
            m_records.insert(parts.at(0), {count, timestamp});
    }
}

QString History::hashItem(const QString &item) const {
    QMessageAuthenticationCode mac(QCryptographicHash::Sha256, m_key.bytes());
    mac.addData(item.toUtf8());
    return QString::fromLatin1(mac.result().toHex());
}

void History::recordUse(const QString &item) {
    if (!m_enabled)
        return;

    Record &record = m_records[hashItem(item)];
    record.count += 1;
    record.timestamp = nowSecs();
    save();
}

void History::save() const {
    if (!m_enabled)
        return;

    QFile file(m_filePath);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text))
        return;

    file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner);

    QTextStream out(&file);
    for (auto it = m_records.constBegin(); it != m_records.constEnd(); ++it)
        out << it.key() << '|' << it->count << '|' << it->timestamp << '\n';
}

quint64 History::score(const QString &item) const {
    if (!m_enabled)
        return 0;

    const auto hit = m_records.constFind(hashItem(item));
    if (hit == m_records.constEnd())
        return 0;

    const quint64 now = nowSecs();
    const quint64 age = now > hit->timestamp ? now - hit->timestamp : 0;
    return static_cast<quint64>(hit->count) * weightForAge(age);
}

void History::sortItems(QStringList &items) const {
    if (!m_enabled)
        return;

    // Alphabetical order breaks score ties, so an unused list stays stable
    // and predictable instead of shuffling on every launch.
    std::sort(items.begin(), items.end(), [this](const QString &a, const QString &b) {
        const quint64 scoreA = score(a);
        const quint64 scoreB = score(b);
        if (scoreA != scoreB)
            return scoreA > scoreB;
        return a < b;
    });
}
