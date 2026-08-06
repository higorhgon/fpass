#pragma once

#include <QString>
#include <QStringList>

// Keeps the items that contain every space-separated term of `query`, in any
// order, case-insensitively — the search both the database list and the entry
// list use.
QStringList filterItems(const QStringList &items, const QString &query);
