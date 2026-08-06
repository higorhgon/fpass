#include "config.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QTextStream>

namespace {

QString home() {
    return QDir::homePath();
}

QString unquote(QString value) {
    value = value.trimmed();
    // Strip an inline comment that follows the value, but never one that
    // lives inside quotes.
    if (!value.startsWith(QLatin1Char('"')) && !value.startsWith(QLatin1Char('\''))) {
        const int hash = value.indexOf(QLatin1Char('#'));
        if (hash >= 0)
            value = value.left(hash).trimmed();
    }

    if (value.size() >= 2
            && ((value.startsWith(QLatin1Char('"')) && value.endsWith(QLatin1Char('"')))
                || (value.startsWith(QLatin1Char('\'')) && value.endsWith(QLatin1Char('\'')))))
        value = value.mid(1, value.size() - 2);

    return value;
}

}

namespace Config {

// ~/.config/omapass, except on a machine that still has the ~/.config/fpass
// this app used to be called: that one keeps being used, so an existing
// history, theme and set of created databases survive the rename untouched.
// Move the directory yourself to adopt the new name.
QString configDir() {
    static const QString directory = []() {
        const QString current = home() + QStringLiteral("/.config/omapass");
        const QString legacy = home() + QStringLiteral("/.config/fpass");
        if (!QDir(current).exists() && QDir(legacy).exists())
            return legacy;
        return current;
    }();
    return directory;
}

QString themesDir() {
    return configDir() + QStringLiteral("/themes");
}

QHash<QString, QString> readFlatToml(const QString &path) {
    QHash<QString, QString> values;

    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
        return values;

    QTextStream in(&file);
    QString section;
    while (!in.atEnd()) {
        const QString line = in.readLine().trimmed();
        if (line.isEmpty() || line.startsWith(QLatin1Char('#')))
            continue;

        if (line.startsWith(QLatin1Char('[')) && line.endsWith(QLatin1Char(']'))) {
            section = line.mid(1, line.size() - 2).trimmed();
            continue;
        }

        const int equals = line.indexOf(QLatin1Char('='));
        if (equals < 0)
            continue;

        const QString key = line.left(equals).trimmed();
        const QString value = unquote(line.mid(equals + 1));
        values.insert(section.isEmpty() ? key : section + QLatin1Char('.') + key, value);
    }

    return values;
}

QString resolveLanguage(const QString &configured, const QString &langEnv, const QString &lcAllEnv) {
    if (configured == QStringLiteral("en") || configured == QStringLiteral("pt-BR"))
        return configured;

    QString envLang = langEnv;
    if (envLang.isEmpty())
        envLang = lcAllEnv;

    envLang = envLang.toLower();
    if (envLang.startsWith(QStringLiteral("pt")))
        return QStringLiteral("pt-BR");
    if (envLang.startsWith(QStringLiteral("en")))
        return QStringLiteral("en");

    return QStringLiteral("en");
}

void ensureConfigExists() {
    QDir().mkpath(themesDir());

    const QString path = configDir() + QStringLiteral("/config.toml");
    if (QFile::exists(path))
        return;

    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Text))
        return;

    QTextStream out(&file);
    out << "[general]\n"
        << "path = \"~/\"\n"
        << "recency = true\n"
        << "theme = \"default\"\n"
        << "language = \"auto\"\n";
}

AppConfig load() {
    AppConfig config;
    config.searchPath = home();

    const QHash<QString, QString> raw = readFlatToml(configDir() + QStringLiteral("/config.toml"));

    QString path = raw.value(QStringLiteral("general.path"));
    if (!path.isEmpty()) {
        if (path.startsWith(QStringLiteral("~/")))
            path = home() + path.mid(1);
        config.searchPath = path;
    }

    const QString recency = raw.value(QStringLiteral("general.recency"));
    if (!recency.isEmpty())
        config.recencyEnabled = recency != QStringLiteral("false");

    const QString theme = raw.value(QStringLiteral("general.theme"));
    if (!theme.isEmpty())
        config.themeName = theme;

    config.language = resolveLanguage(raw.value(QStringLiteral("general.language")),
                                      qEnvironmentVariable("LANG"),
                                      qEnvironmentVariable("LC_ALL"));

    if (config.themeName == QStringLiteral("default"))
        return config;

    // Themes are matched on the `name` inside the file, not the filename, so
    // a theme can be dropped in under any filename.
    const QDir dir(themesDir());
    const auto files = dir.entryInfoList({QStringLiteral("*.toml")}, QDir::Files, QDir::Name);
    for (const QFileInfo &info : files) {
        const QHash<QString, QString> values = readFlatToml(info.absoluteFilePath());
        if (values.value(QStringLiteral("theme.name")) != config.themeName)
            continue;

        for (auto it = values.constBegin(); it != values.constEnd(); ++it) {
            if (!it.key().startsWith(QStringLiteral("colors.")))
                continue;
            config.themeOverrides.insert(it.key().mid(7), it.value());
        }
        break;
    }

    return config;
}

}
