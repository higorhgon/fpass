#include "i18n.h"

#include <QFile>
#include <QHash>
#include <QTextStream>

namespace {

// Catalogue entries keyed by "<key>\n<locale>"; a single flat hash keeps the
// lookup (and the fallback retry below) a plain hash probe.
QHash<QString, QString> catalogue;
QString activeLocale = QStringLiteral("en");

const auto fallbackLocale = QStringLiteral("pt-BR");

QString unquote(QString value) {
    value = value.trimmed();
    if (value.size() >= 2 && value.startsWith(QLatin1Char('"')) && value.endsWith(QLatin1Char('"')))
        value = value.mid(1, value.size() - 2);

    // The catalogue only ever uses the two escapes rust-i18n's YAML produced.
    value.replace(QStringLiteral("\\n"), QStringLiteral("\n"));
    value.replace(QStringLiteral("\\\""), QStringLiteral("\""));
    return value;
}

QString interpolate(QString text, const QVariantMap &args) {
    for (auto it = args.constBegin(); it != args.constEnd(); ++it)
        text.replace(QStringLiteral("%{") + it.key() + QStringLiteral("}"), it.value().toString());
    return text;
}

}

namespace I18n {

// The catalogue is a flat two-level YAML: a key at column zero, then one
// indented line per locale. Parsing it directly avoids a YAML dependency for
// a shape that never varies.
void load(const QString &locale) {
    activeLocale = locale;
    catalogue.clear();

    QFile file(QStringLiteral(":/locales/app.yml"));
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
        return;

    QTextStream in(&file);
    QString currentKey;
    while (!in.atEnd()) {
        const QString line = in.readLine();
        const QString trimmed = line.trimmed();
        if (trimmed.isEmpty() || trimmed.startsWith(QLatin1Char('#')))
            continue;

        const int colon = line.indexOf(QLatin1Char(':'));
        if (colon < 0)
            continue;

        if (!line.startsWith(QLatin1Char(' '))) {
            const QString key = line.left(colon).trimmed();
            // `_version: 2` and anything else carrying a value on the key
            // line is metadata, not a translation group.
            currentKey = line.mid(colon + 1).trimmed().isEmpty() ? key : QString();
            continue;
        }

        if (currentKey.isEmpty())
            continue;

        const QString entryLocale = line.left(colon).trimmed();
        catalogue.insert(currentKey + QLatin1Char('\n') + entryLocale,
                         unquote(line.mid(colon + 1)));
    }
}

QString locale() {
    return activeLocale;
}

QString t(const QString &key) {
    const auto hit = catalogue.constFind(key + QLatin1Char('\n') + activeLocale);
    if (hit != catalogue.constEnd())
        return *hit;

    const auto fallback = catalogue.constFind(key + QLatin1Char('\n') + fallbackLocale);
    if (fallback != catalogue.constEnd())
        return *fallback;

    return key;
}

QString t(const QString &key, const QVariantMap &args) {
    return interpolate(t(key), args);
}

QString t(const QString &key, const QString &argName, const QString &argValue) {
    return interpolate(t(key), {{argName, argValue}});
}

}
