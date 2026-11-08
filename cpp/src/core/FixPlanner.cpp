#include "uvmcheck/core/FixPlanner.h"

#include <cctype>

#include <algorithm>
#include <sstream>

namespace uvmcheck::fix {

namespace {

/// Start offset of every line, plus a sentinel at the end of the text.
std::vector<size_t> lineStarts(const std::string& text) {
    std::vector<size_t> starts{0};
    for (size_t i = 0; i < text.size(); ++i) {
        if (text[i] == '\n' && i + 1 < text.size()) starts.push_back(i + 1);
    }
    starts.push_back(text.size());
    return starts;
}

size_t lineOf(const std::vector<size_t>& starts, size_t offset) {
    // Last line start <= offset (the sentinel is excluded).
    auto it = std::upper_bound(starts.begin(), starts.end() - 1, offset);
    return static_cast<size_t>(it - starts.begin()) - 1;
}

std::vector<std::string> splitLines(const std::string& text) {
    std::vector<std::string> lines;
    size_t pos = 0;
    while (pos < text.size()) {
        size_t nl = text.find('\n', pos);
        if (nl == std::string::npos) {
            lines.push_back(text.substr(pos));
            break;
        }
        lines.push_back(text.substr(pos, nl - pos));
        pos = nl + 1;
    }
    return lines;
}

/// Lines [first, last] of the original, rewritten: a run of edits that share lines.
struct Change {
    size_t first = 0, last = 0;
    std::vector<std::string> newLines;
};

constexpr size_t kContext = 3;

std::string renderDiff(const std::vector<std::string>& oldLines, const std::vector<Change>& changes,
                       const std::string& path) {
    if (changes.empty()) return {};
    std::ostringstream out;
    out << "--- a/" << path << "\n+++ b/" << path << "\n";
    long delta = 0; // new line number minus old line number before the current hunk
    size_t i = 0;
    while (i < changes.size()) {
        // Changes whose context windows touch share one hunk.
        size_t j = i;
        while (j + 1 < changes.size() && changes[j + 1].first <= changes[j].last + 1 + 2 * kContext) ++j;
        const size_t hunkFirst = changes[i].first >= kContext ? changes[i].first - kContext : 0;
        const size_t hunkLast = std::min(oldLines.size() - 1, changes[j].last + kContext);

        std::vector<std::string> body;
        size_t oldCount = 0, newCount = 0;
        size_t line = hunkFirst;
        long hunkDelta = 0;
        for (size_t k = i; k <= j; ++k) {
            for (; line < changes[k].first; ++line, ++oldCount, ++newCount) body.push_back(" " + oldLines[line]);
            for (size_t l = changes[k].first; l <= changes[k].last; ++l, ++oldCount) body.push_back("-" + oldLines[l]);
            for (const auto& nl : changes[k].newLines) body.push_back("+" + nl), ++newCount;
            hunkDelta += static_cast<long>(changes[k].newLines.size()) -
                         static_cast<long>(changes[k].last - changes[k].first + 1);
            line = changes[k].last + 1;
        }
        for (; line <= hunkLast; ++line, ++oldCount, ++newCount) body.push_back(" " + oldLines[line]);

        const long oldStart = static_cast<long>(hunkFirst) + 1;
        const long newStart = oldStart + delta;
        out << "@@ -" << oldStart << "," << oldCount << " +" << (newCount ? newStart : newStart - 1) << ","
            << newCount << " @@\n";
        for (const auto& b : body) out << b << "\n";
        delta += hunkDelta;
        i = j + 1;
    }
    return out.str();
}

} // namespace

FilePlan planEdits(const std::string& original, std::vector<TextEdit> edits, const std::string& displayPath) {
    FilePlan plan;
    std::stable_sort(edits.begin(), edits.end(), [](const TextEdit& a, const TextEdit& b) {
        return a.start != b.start ? a.start < b.start : a.end < b.end;
    });

    // Keep edits in file order; drop exact repeats and anything overlapping
    // an edit already taken. Two insertions at the same point overlap too.
    std::vector<TextEdit> taken;
    for (auto& e : edits) {
        if (e.start > e.end || e.end > original.size()) {
            plan.skipped.push_back(e);
            continue;
        }
        if (!taken.empty()) {
            const auto& prev = taken.back();
            if (prev.start == e.start && prev.end == e.end && prev.text == e.text) {
                ++plan.duplicates;
                continue;
            }
            if (e.start < prev.end || (e.start == prev.start)) {
                plan.skipped.push_back(e);
                continue;
            }
        }
        taken.push_back(std::move(e));
    }
    plan.applied = taken.size();

    // Build the new text and the per-line changes in one pass.
    const auto starts = lineStarts(original);
    const auto oldLines = splitLines(original);
    std::vector<Change> changes;
    size_t cursor = 0;
    size_t k = 0;
    while (k < taken.size()) {
        // A run of edits whose lines touch rewrites lines [first, last].
        const size_t first = lineOf(starts, taken[k].start);
        size_t last = lineOf(starts, taken[k].end > taken[k].start ? taken[k].end - 1 : taken[k].start);
        size_t m = k;
        while (m + 1 < taken.size() && lineOf(starts, taken[m + 1].start) <= last) {
            ++m;
            const auto& e = taken[m];
            last = std::max(last, lineOf(starts, e.end > e.start ? e.end - 1 : e.start));
        }
        const size_t blockStart = starts[first];
        const size_t blockEnd = std::min(original.size(), starts[last + 1]);

        plan.updated.append(original, cursor, blockStart - cursor);
        std::string block;
        size_t pos = blockStart;
        for (size_t x = k; x <= m; ++x) {
            block.append(original, pos, taken[x].start - pos);
            block += taken[x].text;
            pos = taken[x].end;
        }
        block.append(original, pos, blockEnd - pos);
        plan.updated += block;
        cursor = blockEnd;

        Change c;
        c.first = first;
        c.last = std::min(last, oldLines.empty() ? 0 : oldLines.size() - 1);
        c.newLines = splitLines(block);
        const bool same = c.last - c.first + 1 == c.newLines.size() &&
                          std::equal(c.newLines.begin(), c.newLines.end(), oldLines.begin() + c.first);
        if (!same) changes.push_back(std::move(c));
        k = m + 1;
    }
    plan.updated.append(original, cursor, std::string::npos);

    if (plan.updated != original && !oldLines.empty()) plan.diff = renderDiff(oldLines, changes, displayPath);
    return plan;
}

bool dropsMacroCall(std::string_view text, size_t start, size_t end, const std::string& replacement) {
    if (start >= end || end > text.size()) return false;
    const std::string_view span = text.substr(start, end - start);
    for (size_t pos = span.find('`'); pos != std::string_view::npos; pos = span.find('`', pos + 1)) {
        size_t stop = pos + 1;
        while (stop < span.size() && (std::isalnum(static_cast<unsigned char>(span[stop])) || span[stop] == '_')) {
            ++stop;
        }
        if (stop == pos + 1) continue; // `` or `" inside a macro body
        if (replacement.find(span.substr(pos, stop - pos)) == std::string::npos) return true;
    }
    return false;
}

} // namespace uvmcheck::fix
