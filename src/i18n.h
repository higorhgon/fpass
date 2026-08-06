#pragma once

#include <QObject>
#include <QString>
#include <QVariantMap>

// Translation layer over `locales/app.yml`, the same catalogue the Rust TUI
// used. The file ships inside the binary's resources, so the running app
// never depends on where it was installed from.
namespace I18n {

void load(const QString &locale);
QString locale();

QString t(const QString &key);
QString t(const QString &key, const QVariantMap &args);
QString t(const QString &key, const QString &argName, const QString &argValue);

}

// QML-side face of the catalogue, exposed as the `i18n` context property.
class Strings : public QObject {
    Q_OBJECT

public:
    using QObject::QObject;

    // Trimmed on the way out: several of these strings were box titles in
    // the TUI and carry padding spaces (or a leading newline) that only made
    // sense inside a drawn border.
    Q_INVOKABLE QString t(const QString &key) const { return I18n::t(key).trimmed(); }
    Q_INVOKABLE QString t(const QString &key, const QVariantMap &args) const {
        return I18n::t(key, args).trimmed();
    }
};
