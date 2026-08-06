#include "process.h"

#include <QProcess>

ProcResult runProcess(const QString &program, const QStringList &args,
                      const QByteArray &stdinData, const QProcessEnvironment &env) {
    ProcResult result;

    QProcess process;
    if (!env.isEmpty())
        process.setProcessEnvironment(env);
    process.start(program, args);

    if (!process.waitForStarted(-1)) {
        result.err = process.errorString();
        return result;
    }
    result.started = true;

    if (!stdinData.isEmpty())
        process.write(stdinData);
    process.closeWriteChannel();

    process.waitForFinished(-1);

    result.success = process.exitStatus() == QProcess::NormalExit && process.exitCode() == 0;
    result.out = QString::fromUtf8(process.readAllStandardOutput());
    result.err = QString::fromUtf8(process.readAllStandardError());
    return result;
}
