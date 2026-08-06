#include "filter.h"

QStringList filterItems(const QStringList &items, const QString &query) {
    const QStringList terms = query.toLower().split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (terms.isEmpty())
        return items;

    QStringList matches;
    for (const QString &item : items) {
        const QString lower = item.toLower();
        const bool matchesAll = std::all_of(terms.cbegin(), terms.cend(),
                                            [&lower](const QString &term) {
            return lower.contains(term);
        });
        if (matchesAll)
            matches.append(item);
    }
    return matches;
}
