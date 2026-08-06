#include "clipboard.h"

#include "process.h"

#include <QTimer>

namespace {

#if defined(Q_OS_MACOS)
const auto copyCommand = QStringLiteral("pbcopy");
#else
const auto copyCommand = QStringLiteral("wl-copy");
#endif

void clearClipboard() {
#if defined(Q_OS_MACOS)
    runProcess(copyCommand, {}, QByteArray());
#else
    // `--clear` tells the compositor to drop the selection outright. Writing
    // an empty string instead leaves some compositors holding the old value,
    // since they do not treat it as a new selection.
    runProcess(copyCommand, {QStringLiteral("--clear")});
#endif
}

}

namespace Clipboard {

bool copy(const QString &text) {
    QStringList args;
#if !defined(Q_OS_MACOS)
    // Marks the content as sensitive for wl-clipboard (>= 2.2.1), which then
    // advertises the `x-kde-passwordManagerHint` mime type. Clipboard
    // managers such as cliphist honour it and keep the value out of their
    // on-disk history — fixing the leak at the source rather than deleting
    // the password after it was already written down.
    args << QStringLiteral("--sensitive");
#endif

    const ProcResult result = runProcess(copyCommand, args, text.toUtf8());
    return result.started && result.success;
}

void scheduleClear(const Secret &password) {
    const Secret copyOfPassword = password;
    QTimer::singleShot(clearSecs * 1000, [copyOfPassword]() {
        clearClipboard();
#if !defined(Q_OS_MACOS)
        // Fallback for wl-clipboard < 2.2.1, or a clipboard manager that
        // ignores the sensitive hint: try to delete the entry from the
        // persisted history. A no-op (and harmless) when cliphist is absent
        // or the hint already kept the value from being stored.
        runProcess(QStringLiteral("cliphist"),
                   {QStringLiteral("delete-query"), copyOfPassword.toString()});
#endif
    });
}

}
