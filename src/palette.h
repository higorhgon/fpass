#pragma once

#include <QColor>
#include <QFileSystemWatcher>
#include <QHash>
#include <QObject>
#include <QString>

// The colours the interface draws with, resolved live from the Omarchy theme
// (~/.local/state/omarchy/current/theme/colors.toml) and re-read whenever the
// desktop switches themes.
//
// The semantic role names are the ones omapass has always used in its own theme
// files (Title, Base, Guidance, …), so a `~/.config/fpass/themes/*.toml`
// still pins whatever it pins — those overrides are applied last and win
// over the Omarchy palette.
class Palette : public QObject {
    Q_OBJECT
    Q_PROPERTY(QColor background READ background NOTIFY changed)
    Q_PROPERTY(QColor surface READ surface NOTIFY changed)
    Q_PROPERTY(QColor selection READ selection NOTIFY changed)
    Q_PROPERTY(QColor base READ base NOTIFY changed)
    Q_PROPERTY(QColor title READ title NOTIFY changed)
    Q_PROPERTY(QColor guidance READ guidance NOTIFY changed)
    Q_PROPERTY(QColor annotation READ annotation NOTIFY changed)
    Q_PROPERTY(QColor important READ important NOTIFY changed)
    Q_PROPERTY(QColor alertInfo READ alertInfo NOTIFY changed)
    Q_PROPERTY(QColor alertWarn READ alertWarn NOTIFY changed)
    Q_PROPERTY(QColor alertError READ alertError NOTIFY changed)
    Q_PROPERTY(bool darkMode READ darkMode NOTIFY darkModeChanged)

public:
    explicit Palette(const QHash<QString, QString> &overrides, QObject *parent = nullptr);

    QColor background() const { return m_background; }
    QColor surface() const { return m_surface; }
    QColor selection() const { return m_selection; }
    QColor base() const { return m_base; }
    QColor title() const { return m_title; }
    QColor guidance() const { return m_guidance; }
    QColor annotation() const { return m_annotation; }
    QColor important() const { return m_important; }
    QColor alertInfo() const { return m_alertInfo; }
    QColor alertWarn() const { return m_alertWarn; }
    QColor alertError() const { return m_alertError; }
    bool darkMode() const { return m_darkMode; }

public slots:
    // Called when the desktop's light/dark preference changes. Only decides
    // the built-in fallback palette — an Omarchy theme, when present, states
    // its own mode and wins.
    void setSystemDarkMode(bool darkMode);

signals:
    void changed();
    void darkModeChanged();

private:
    void reload();
    void watchThemeFiles();

    QHash<QString, QString> m_overrides;

    QColor m_background;
    QColor m_surface;
    QColor m_selection;
    QColor m_base;
    QColor m_title;
    QColor m_guidance;
    QColor m_annotation;
    QColor m_important;
    QColor m_alertInfo;
    QColor m_alertWarn;
    QColor m_alertError;

    bool m_darkMode = true;
    bool m_systemDarkMode = true;
    QFileSystemWatcher m_watcher;
};
