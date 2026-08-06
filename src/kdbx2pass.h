#pragma once

#include <QString>
#include <QStringList>
#include <QVector>

// Imports a KeePassXC database (.kdbx) into a pass store, encrypting every
// entry to the given GPG recipients. Runs headless, from the command line.
//
// No temporary file is written: the exported CSV — which holds every password
// in the clear — only ever lives in memory, and each row's password cell is
// wiped as soon as it has been used, so no password outlives the processing
// of its own entry.
namespace Kdbx2Pass {

// Returns an empty string on success, or the error to print.
QString run(const QString &dbPath, const QStringList &recipients);

// Exposed for testing.
QVector<QStringList> parseCsv(const QString &content);
QString sanitizeTitle(const QString &title);

}
