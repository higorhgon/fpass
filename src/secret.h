#pragma once

#include <QByteArray>
#include <QString>

// A secret held in memory that overwrites its own storage on the way out.
// QString and QByteArray are implicitly shared, so handing one around leaves
// copies of the plaintext behind that nobody owns; Secret always keeps a
// deep, uniquely-owned buffer and wipes it in the destructor. That is the
// strongest guarantee available without a custom allocator — it cannot undo
// a copy that Qt made before the value reached us.
class Secret {
public:
    Secret() = default;
    explicit Secret(const QString &text);
    explicit Secret(const QByteArray &bytes);
    Secret(const Secret &other);
    Secret &operator=(const Secret &other);
    ~Secret();

    bool isEmpty() const { return m_data.isEmpty(); }
    int length() const { return m_data.size(); }

    QString toString() const { return QString::fromUtf8(m_data); }
    const QByteArray &bytes() const { return m_data; }

    void clear();

private:
    void adopt(const char *data, int size);
    void wipe();

    QByteArray m_data;
};
