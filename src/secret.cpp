#include "secret.h"

#include <cstring>

Secret::Secret(const QString &text) {
    const QByteArray utf8 = text.toUtf8();
    adopt(utf8.constData(), utf8.size());
}

Secret::Secret(const QByteArray &bytes) {
    adopt(bytes.constData(), bytes.size());
}

Secret::Secret(const Secret &other) {
    adopt(other.m_data.constData(), other.m_data.size());
}

Secret &Secret::operator=(const Secret &other) {
    if (this == &other)
        return *this;

    wipe();
    adopt(other.m_data.constData(), other.m_data.size());
    return *this;
}

Secret::~Secret() {
    wipe();
}

void Secret::clear() {
    wipe();
    m_data.clear();
}

// Build the buffer from raw bytes rather than assigning a QByteArray, so the
// data is ours alone and wipe() is guaranteed to hit the only copy.
void Secret::adopt(const char *data, int size) {
    m_data = QByteArray(data, size);
}

void Secret::wipe() {
    if (m_data.isEmpty())
        return;

    std::memset(m_data.data(), 0, static_cast<size_t>(m_data.size()));
}
