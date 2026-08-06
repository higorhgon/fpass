#pragma once

#include <QByteArray>
#include <QProcessEnvironment>
#include <QString>
#include <QStringList>

// Outcome of an external command. `started` separates "the binary is not
// installed" from "the command ran and failed", which callers report
// differently.
struct ProcResult {
    bool started = false;
    bool success = false;
    QString out;
    QString err;
};

ProcResult runProcess(const QString &program, const QStringList &args,
                      const QByteArray &stdinData = QByteArray(),
                      const QProcessEnvironment &env = QProcessEnvironment());
