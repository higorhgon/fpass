#include "kdbx2pass.h"

#include "i18n.h"
#include "process.h"
#include "secret.h"

#include <QTextStream>

#include <cstdio>

#if defined(Q_OS_UNIX)
#include <termios.h>
#include <unistd.h>
#endif

namespace {

// Root group names KeePassXC uses or generates (it varies by version and
// locale — a pt-BR install calls the root group "Senhas"). Entries in these
// go straight to the store's root instead of into a folder of that name.
const QStringList rootGroupNames = {QString(), QStringLiteral("Principal"), QStringLiteral("Root"),
                                    QStringLiteral("Senhas"), QStringLiteral("/")};

QTextStream &stdOut() {
    static QTextStream stream(stdout);
    return stream;
}

Secret promptPassword(const QString &prompt) {
    stdOut() << prompt;
    stdOut().flush();

#if defined(Q_OS_UNIX)
    termios original;
    const bool isTerminal = isatty(STDIN_FILENO) && tcgetattr(STDIN_FILENO, &original) == 0;
    if (isTerminal) {
        termios quiet = original;
        quiet.c_lflag &= ~static_cast<tcflag_t>(ECHO);
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &quiet);
    }
#endif

    QTextStream in(stdin);
    const QString line = in.readLine();

#if defined(Q_OS_UNIX)
    if (isTerminal) {
        tcsetattr(STDIN_FILENO, TCSAFLUSH, &original);
        stdOut() << '\n';
        stdOut().flush();
    }
#endif

    return Secret(line);
}

QString runPassCommand(const QStringList &args, const QByteArray &stdinData) {
    const ProcResult result = runProcess(QStringLiteral("pass"), args, stdinData);
    if (!result.started)
        return I18n::t(QStringLiteral("pass.spawn_error"), QStringLiteral("err"), result.err);
    if (!result.success) {
        const QString reported = result.err.trimmed();
        return reported.isEmpty()
            ? I18n::t(QStringLiteral("kdbx2pass.pass_cmd_failed"), QStringLiteral("cmd"),
                      args.join(QLatin1Char(' ')))
            : reported;
    }
    return QString();
}

QString exportKdbxCsv(const QString &dbPath, const Secret &password, Secret *csv) {
    QByteArray stdinData = password.bytes();
    stdinData.append('\n');

    const ProcResult result = runProcess(QStringLiteral("keepassxc-cli"),
                                         {QStringLiteral("export"), QStringLiteral("--format"),
                                          QStringLiteral("csv"), dbPath},
                                         stdinData);
    stdinData.fill('\0');

    if (!result.started)
        return I18n::t(QStringLiteral("keepass.spawn_error"), QStringLiteral("err"), result.err);
    if (!result.success)
        return result.err.trimmed();

    *csv = Secret(result.out);
    return QString();
}

QString cell(const QStringList &row, int index) {
    return index >= 0 ? row.value(index) : QString();
}

}

namespace Kdbx2Pass {

// RFC4180 CSV: double quotes wrap fields containing commas or newlines, and
// `""` inside a quoted field is a literal quote. Hand-written rather than
// pulling in a dependency for one caller — KeePassXC's exported Notes is the
// one field that routinely holds commas and line breaks, so it has to be
// right.
QVector<QStringList> parseCsv(const QString &content) {
    QVector<QStringList> rows;
    QStringList row;
    QString field;
    bool inQuotes = false;

    for (int i = 0; i < content.size(); ++i) {
        const QChar c = content.at(i);

        if (inQuotes) {
            if (c == QLatin1Char('"')) {
                if (i + 1 < content.size() && content.at(i + 1) == QLatin1Char('"')) {
                    field.append(QLatin1Char('"'));
                    ++i;
                } else {
                    inQuotes = false;
                }
            } else {
                field.append(c);
            }
            continue;
        }

        if (c == QLatin1Char('"')) {
            inQuotes = true;
        } else if (c == QLatin1Char(',')) {
            row.append(field);
            field.clear();
        } else if (c == QLatin1Char('\r')) {
            // Ignored: line endings are decided by the \n alone.
        } else if (c == QLatin1Char('\n')) {
            row.append(field);
            field.clear();
            rows.append(row);
            row.clear();
        } else {
            field.append(c);
        }
    }

    if (!field.isEmpty() || !row.isEmpty()) {
        row.append(field);
        rows.append(row);
    }

    return rows;
}

// Replaces anything outside [unicode letters/digits, `_-. `] with `_`,
// mirroring the `re.sub(r'[^\w\-. ]', '_', title)` of the original script.
QString sanitizeTitle(const QString &title) {
    QString safe;
    safe.reserve(title.size());
    for (const QChar c : title) {
        const bool keep = c.isLetterOrNumber() || c == QLatin1Char('_') || c == QLatin1Char('-')
            || c == QLatin1Char('.') || c == QLatin1Char(' ');
        safe.append(keep ? c : QLatin1Char('_'));
    }

    const QString trimmed = safe.trimmed();
    return trimmed.isEmpty() ? I18n::t(QStringLiteral("kdbx2pass.no_title")) : trimmed;
}

QString run(const QString &dbPath, const QStringList &recipients) {
    if (recipients.isEmpty())
        return I18n::t(QStringLiteral("cli.kdbx2pass_usage"));

    // 1. Point the password store at the right recipients.
    QString error = runPassCommand(QStringList{QStringLiteral("init")} + recipients, QByteArray());
    if (!error.isEmpty())
        return error;

    // 2. Export the kdbx to CSV, asking for the master password once.
    const Secret password = promptPassword(
        I18n::t(QStringLiteral("kdbx2pass.master_password_prompt"), QStringLiteral("db"), dbPath));

    Secret csv;
    error = exportKdbxCsv(dbPath, password, &csv);
    if (!error.isEmpty())
        return error;

    // 3. Parse and import each entry.
    QVector<QStringList> rows = parseCsv(csv.toString());
    csv.clear();
    if (rows.isEmpty())
        return I18n::t(QStringLiteral("kdbx2pass.empty_csv"));

    const QStringList header = rows.takeFirst();
    const int groupColumn = header.indexOf(QStringLiteral("Group"));
    const int titleColumn = header.indexOf(QStringLiteral("Title"));
    const int usernameColumn = header.indexOf(QStringLiteral("Username"));
    const int passwordColumn = header.indexOf(QStringLiteral("Password"));
    const int urlColumn = header.indexOf(QStringLiteral("URL"));
    const int notesColumn = header.indexOf(QStringLiteral("Notes"));

    for (QStringList &row : rows) {
        QString rawGroup = cell(row, groupColumn);
        while (rawGroup.startsWith(QLatin1Char('/')))
            rawGroup.remove(0, 1);
        while (rawGroup.endsWith(QLatin1Char('/')))
            rawGroup.chop(1);
        rawGroup.replace(QStringLiteral("/./"), QStringLiteral("/"));

        QString title = cell(row, titleColumn).trimmed();
        if (title.isEmpty())
            title = I18n::t(QStringLiteral("kdbx2pass.no_title"));

        const QString username = cell(row, usernameColumn);
        const Secret entryPassword(cell(row, passwordColumn));
        // The plaintext copy inside `row` has been taken above; wipe it now
        // so the password does not sit in unprotected memory for the rest of
        // the import (`rows` only goes away at the end).
        if (passwordColumn >= 0 && passwordColumn < row.size())
            row[passwordColumn].fill(QLatin1Char('\0'));

        const QString url = cell(row, urlColumn);
        const QString notes = cell(row, notesColumn);

        QStringList segments = rawGroup.isEmpty() ? QStringList()
                                                  : rawGroup.split(QLatin1Char('/'));
        if (!segments.isEmpty() && rootGroupNames.contains(segments.first()))
            segments.removeFirst();

        const QString group = segments.join(QLatin1Char('/'));
        const QString safeTitle = sanitizeTitle(title);
        const QString entryPath = group.isEmpty() ? safeTitle
                                                  : group + QLatin1Char('/') + safeTitle;

        // The multi-line shape pass expects: line 1 the password, the rest
        // metadata.
        QByteArray content = entryPassword.bytes();
        if (!username.isEmpty())
            content.append(QStringLiteral("\nusername: %1").arg(username).toUtf8());
        if (!url.isEmpty())
            content.append(QStringLiteral("\nurl: %1").arg(url).toUtf8());
        if (!notes.isEmpty())
            content.append(QStringLiteral("\nnotes: %1").arg(notes).toUtf8());

        stdOut() << I18n::t(QStringLiteral("kdbx2pass.importing"), QStringLiteral("entry"), entryPath)
                 << '\n';
        stdOut().flush();

        error = runPassCommand({QStringLiteral("insert"), QStringLiteral("-m"),
                                QStringLiteral("-f"), entryPath},
                               content);
        content.fill('\0');
        if (!error.isEmpty())
            return error;
    }

    stdOut() << I18n::t(QStringLiteral("kdbx2pass.done")) << '\n';
    stdOut().flush();
    return QString();
}

}
