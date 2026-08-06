#pragma once

#include <QHash>
#include <QString>

struct AppConfig {
    QString searchPath;
    bool recencyEnabled = true;
    QString themeName = QStringLiteral("default");
    QString language = QStringLiteral("en");
    // Role name ("Title", "Base", …) to hex colour, read from the active
    // theme in ~/.config/fpass/themes. These win over the Omarchy palette,
    // so a user who pinned specific colours keeps them.
    QHash<QString, QString> themeOverrides;
};

namespace Config {

QString configDir();
QString themesDir();

void ensureConfigExists();
AppConfig load();

// Exposed for testing: resolves the effective UI language from the config
// value and the environment, in that order of priority.
QString resolveLanguage(const QString &configured, const QString &langEnv, const QString &lcAllEnv);

// Minimal TOML reader for the flat `[section] key = "value"` files fpass and
// Omarchy both use. Keys come back as "section.key" (or bare "key" outside a
// section). Not a general TOML parser — it does not need to be.
QHash<QString, QString> readFlatToml(const QString &path);

}
