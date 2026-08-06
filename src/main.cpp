#include <QCoreApplication>
#include <QFile>
#include <QFont>
#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QQuickStyle>
#include <QTextStream>
#include <QUrl>

#include "appcontroller.h"
#include "config.h"
#include "i18n.h"
#include "kdbx2pass.h"
#include "palette.h"
#include "systemtheme.h"

namespace {

const auto applicationVersion = QStringLiteral("2.0.0");

void printHelp(QTextStream &out) {
    out << I18n::t(QStringLiteral("cli.help_title")) << "\n\n"
        << I18n::t(QStringLiteral("cli.help_usage_header")) << '\n'
        << I18n::t(QStringLiteral("cli.help_usage_line")) << '\n'
        << "  omapass --kdbx2pass <banco.kdbx> <KEYID1> [KEYID2...]\n\n"
        << I18n::t(QStringLiteral("cli.help_options_header")) << '\n'
        << "  -v, --version      " << I18n::t(QStringLiteral("cli.help_version")) << '\n'
        << "  -h, --help         " << I18n::t(QStringLiteral("cli.help_help")) << '\n'
        << "  --kdbx2pass        " << I18n::t(QStringLiteral("cli.help_kdbx2pass_1")) << '\n'
        << "                     " << I18n::t(QStringLiteral("cli.help_kdbx2pass_2")) << '\n';
}

// The command-line modes never open a window, so they run under a plain
// QCoreApplication and return before any GUI is constructed.
int runCommandLine(int argc, char *argv[], const QStringList &args, bool *handled) {
    *handled = true;

    QCoreApplication app(argc, argv);
    QTextStream out(stdout);
    QTextStream err(stderr);

    const QString command = args.at(1);
    if (command == QStringLiteral("-v") || command == QStringLiteral("--version")) {
        out << I18n::t(QStringLiteral("cli.version"), QStringLiteral("version"), applicationVersion)
            << '\n';
        return 0;
    }

    if (command == QStringLiteral("-h") || command == QStringLiteral("--help")) {
        printHelp(out);
        return 0;
    }

    if (command == QStringLiteral("--kdbx2pass")) {
        if (args.size() < 4) {
            err << I18n::t(QStringLiteral("cli.kdbx2pass_usage")) << '\n';
            return 1;
        }

        const QString error = Kdbx2Pass::run(args.at(2), args.mid(3));
        if (!error.isEmpty()) {
            err << "omapass --kdbx2pass: " << error << '\n';
            return 1;
        }
        return 0;
    }

    *handled = false;
    return 0;
}

}

int main(int argc, char *argv[]) {
    Config::ensureConfigExists();
    const AppConfig config = Config::load();
    I18n::load(config.language);

    QStringList args;
    args.reserve(argc);
    for (int i = 0; i < argc; ++i)
        args.append(QString::fromLocal8Bit(argv[i]));

    if (args.size() > 1 && args.at(1).startsWith(QLatin1Char('-'))) {
        bool handled = false;
        const int status = runCommandLine(argc, argv, args, &handled);
        if (handled)
            return status;
    }

    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("omapass"));
    app.setApplicationVersion(applicationVersion);
    app.setOrganizationName(QStringLiteral("omapass"));
    app.setDesktopFileName(QStringLiteral("omapass"));
    app.setWindowIcon(QIcon::fromTheme(QStringLiteral("omapass")));

    // Basic rather than Material: every control here is drawn against the
    // Omarchy palette, and Material's own chrome (filled text containers,
    // floating placeholder labels) fights that instead of helping.
    QQuickStyle::setStyle(QStringLiteral("Basic"));

    SystemTheme systemTheme(&app);
    Palette palette(config.themeOverrides, &app);
    palette.setSystemDarkMode(systemTheme.darkMode());
    QObject::connect(&systemTheme, &SystemTheme::darkModeChanged, &palette,
                     &Palette::setSystemDarkMode);

    // The whole interface inherits this one font, so nothing names a family
    // of its own. "monospace" is the generic family fontconfig resolves, and
    // `omarchy font set` writes exactly that rule into
    // ~/.config/fontconfig/fonts.conf — so the desktop's chosen monospace is
    // what omapass draws with. Its size follows the desktop text scale, which
    // keeps the chrome (menus, dialogs) growing with the rest.
    const QFont interfaceFont(QStringLiteral("monospace"));
    const qreal basePointSize = interfaceFont.pointSizeF() > 0 ? interfaceFont.pointSizeF()
                                                               : app.font().pointSizeF();
    const auto applyInterfaceFont = [&app, interfaceFont, basePointSize](qreal textScale) {
        QFont scaled = interfaceFont;
        scaled.setPointSizeF(basePointSize * textScale);
        app.setFont(scaled);
    };
    applyInterfaceFont(systemTheme.textScale());
    QObject::connect(&systemTheme, &SystemTheme::textScaleChanged, &app,
                     [applyInterfaceFont](qreal textScale) { applyInterfaceFont(textScale); });

    AppController controller(config);
    Strings strings;

    QQmlApplicationEngine engine;
    QObject::connect(&engine, &QQmlApplicationEngine::warnings, &app,
                     [](const QList<QQmlError> &warnings) {
        for (const QQmlError &warning : warnings)
            qWarning().noquote() << warning.toString();
    });

    engine.rootContext()->setContextProperty(QStringLiteral("controller"), &controller);
    // Deliberately not named "palette": Qt Quick Controls already give every
    // control a `palette` property, which would shadow the context property
    // inside them.
    engine.rootContext()->setContextProperty(QStringLiteral("theme"), &palette);
    engine.rootContext()->setContextProperty(QStringLiteral("systemTheme"), &systemTheme);
    engine.rootContext()->setContextProperty(QStringLiteral("i18n"), &strings);

    engine.load(QUrl(QStringLiteral("qrc:/Main.qml")));
    if (engine.rootObjects().isEmpty()) {
        qCritical() << "Could not load the omapass interface; resource available:"
                    << QFile::exists(QStringLiteral(":/Main.qml"));
        return -1;
    }

    return app.exec();
}
