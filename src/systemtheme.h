// Desktop light/dark preference and text scale, read from the freedesktop
// portal. Taken from Omacalc (https://github.com/omacom-io/omacalc), MIT
// licensed, copyright David Heinemeier Hansson, so that the whole Omarchy
// app family reacts to the desktop identically.
#pragma once

#include <QObject>

#include <functional>

class QDBusVariant;

class SystemTheme : public QObject {
    Q_OBJECT
    // Declared as properties (Omacalc reads them from C++ only) because the
    // fpass interface sizes itself against the text scale directly.
    Q_PROPERTY(bool darkMode READ darkMode NOTIFY darkModeChanged)
    Q_PROPERTY(qreal textScale READ textScale NOTIFY textScaleChanged)

public:
    explicit SystemTheme(QObject *parent = nullptr);

    bool darkMode() const { return m_darkMode; }
    qreal textScale() const { return m_textScale; }

signals:
    void darkModeChanged(bool darkMode);
    void textScaleChanged(qreal textScale);

public slots:
    void refresh();

private slots:
    void handlePortalSettingChanged(const QString &nameSpace, const QString &key,
                                    const QDBusVariant &value);

private:
    void requestPortalSetting(const QString &nameSpace, const QString &key,
                              std::function<void(const QVariant &)> handler);
    void requestPortalDarkMode();
    void requestPortalTextScale();
    bool qtDarkMode(bool *known) const;
    void setDarkMode(bool darkMode);
    void setTextScale(qreal textScale);

    bool m_darkMode = true;
    qreal m_textScale = 1.0;
};
