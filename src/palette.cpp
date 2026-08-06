#include "palette.h"

#include "config.h"

#include <QDir>
#include <QFile>

namespace {

QString omarchyCurrentDir() {
    return QDir::homePath() + QStringLiteral("/.local/state/omarchy/current");
}

QString omarchyThemeDir() {
    return omarchyCurrentDir() + QStringLiteral("/theme");
}

QString omarchyColorsPath() {
    return omarchyThemeDir() + QStringLiteral("/colors.toml");
}

// Catppuccin Mocha / Latte, the palettes fpass shipped before it followed the
// desktop. Only reached when no Omarchy theme is installed.
struct Fallback {
    QColor background, surface, selection, base, title, guidance;
    QColor annotation, important, alertInfo, alertWarn, alertError;
};

Fallback fallbackPalette(bool dark) {
    if (dark) {
        return {QColor("#1e1e2e"), QColor("#313244"), QColor("#45475a"), QColor("#cdd6f4"),
                QColor("#74c7ec"), QColor("#7f849c"), QColor("#cba6f7"), QColor("#eba0ac"),
                QColor("#a6e3a1"), QColor("#f9e2af"), QColor("#f38ba8")};
    }
    return {QColor("#eff1f5"), QColor("#dce0e8"), QColor("#ccd0da"), QColor("#4c4f69"),
            QColor("#209fb5"), QColor("#8c8fa1"), QColor("#8839ef"), QColor("#e64553"),
            QColor("#40a02b"), QColor("#df8e1d"), QColor("#d20f39")};
}

// Take the first key that the theme actually defines; Omarchy themes vary in
// which optional colours they carry.
QColor pick(const QHash<QString, QString> &colors, const QStringList &keys, QColor fallback) {
    for (const QString &key : keys) {
        const QColor color(colors.value(key));
        if (color.isValid())
            return color;
    }
    return fallback;
}

bool isDarkColor(const QColor &color) {
    return 0.299 * color.redF() + 0.587 * color.greenF() + 0.114 * color.blueF() < 0.5;
}

}

Palette::Palette(const QHash<QString, QString> &overrides, QObject *parent)
    : QObject(parent), m_overrides(overrides) {
    reload();
    watchThemeFiles();

    // Omarchy replaces the whole `current/theme` symlink on a theme switch,
    // so the directory above the file has to be watched too — and the watch
    // list is rebuilt each time, since the old paths stop existing.
    connect(&m_watcher, &QFileSystemWatcher::fileChanged, this, [this]() {
        reload();
        watchThemeFiles();
    });
    connect(&m_watcher, &QFileSystemWatcher::directoryChanged, this, [this]() {
        reload();
        watchThemeFiles();
    });
}

void Palette::setSystemDarkMode(bool darkMode) {
    if (m_systemDarkMode == darkMode)
        return;

    m_systemDarkMode = darkMode;
    reload();
}

void Palette::reload() {
    const QHash<QString, QString> colors = Config::readFlatToml(omarchyColorsPath());

    // The theme's own `mode` is authoritative; failing that, the background's
    // luminance says it plainly. Only a themeless desktop falls back to the
    // portal's preference.
    const QString mode = colors.value(QStringLiteral("mode"));
    const QColor themeBackground(colors.value(QStringLiteral("background")));
    bool dark = m_systemDarkMode;
    if (mode == QStringLiteral("dark"))
        dark = true;
    else if (mode == QStringLiteral("light"))
        dark = false;
    else if (themeBackground.isValid())
        dark = isDarkColor(themeBackground);

    const Fallback fallback = fallbackPalette(dark);

    m_background = pick(colors, {QStringLiteral("background")}, fallback.background);
    m_surface = pick(colors, {QStringLiteral("lighter_background"), QStringLiteral("dark_background")},
                     fallback.surface);
    m_selection = pick(colors, {QStringLiteral("selection")}, fallback.selection);
    m_base = pick(colors, {QStringLiteral("foreground")}, fallback.base);
    m_title = pick(colors, {QStringLiteral("accent"), QStringLiteral("blue")}, fallback.title);
    m_guidance = pick(colors, {QStringLiteral("muted"), QStringLiteral("dark_foreground")},
                      fallback.guidance);
    m_annotation = pick(colors, {QStringLiteral("magenta")}, fallback.annotation);
    m_important = pick(colors, {QStringLiteral("orange"), QStringLiteral("red")}, fallback.important);
    m_alertInfo = pick(colors, {QStringLiteral("green")}, fallback.alertInfo);
    m_alertWarn = pick(colors, {QStringLiteral("yellow")}, fallback.alertWarn);
    m_alertError = pick(colors, {QStringLiteral("red")}, fallback.alertError);

    const auto applyOverride = [this](const QString &role, QColor &target) {
        const QColor color(m_overrides.value(role));
        if (color.isValid())
            target = color;
    };
    applyOverride(QStringLiteral("Base"), m_base);
    applyOverride(QStringLiteral("Title"), m_title);
    applyOverride(QStringLiteral("Guidance"), m_guidance);
    applyOverride(QStringLiteral("Annotation"), m_annotation);
    applyOverride(QStringLiteral("Important"), m_important);
    applyOverride(QStringLiteral("AlertInfo"), m_alertInfo);
    applyOverride(QStringLiteral("AlertWarn"), m_alertWarn);
    applyOverride(QStringLiteral("AlertError"), m_alertError);

    if (dark != m_darkMode) {
        m_darkMode = dark;
        emit darkModeChanged();
    }

    emit changed();
}

void Palette::watchThemeFiles() {
    const QStringList watched = m_watcher.files() + m_watcher.directories();
    if (!watched.isEmpty())
        m_watcher.removePaths(watched);

    if (QDir(omarchyCurrentDir()).exists())
        m_watcher.addPath(omarchyCurrentDir());
    if (QDir(omarchyThemeDir()).exists())
        m_watcher.addPath(omarchyThemeDir());
    if (QFile::exists(omarchyColorsPath()))
        m_watcher.addPath(omarchyColorsPath());
}
