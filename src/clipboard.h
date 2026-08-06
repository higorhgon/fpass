#pragma once

#include <QString>

#include "secret.h"

namespace Clipboard {

// Seconds a copied password stays on the clipboard. Drives both the timer
// and the countdown the interface shows.
const int clearSecs = 10;

bool copy(const QString &text);

// Wipes the clipboard once `clearSecs` have passed, on the event loop rather
// than a thread.
void scheduleClear(const Secret &password);

}
