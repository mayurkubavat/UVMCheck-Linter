#pragma once

#include <cstddef>
#include <string>
#include <vector>

namespace uvmcheck::fix {

/// One replacement in a file, as byte offsets into its text.
struct TextEdit {
    size_t start = 0;
    size_t end = 0;
    std::string text;
    std::string ruleId;
};

/// The result of applying a file's edits.
struct FilePlan {
    std::string updated;          ///< the text with every applied edit
    size_t applied = 0;           ///< edits applied
    size_t duplicates = 0;        ///< identical edits reported more than once (applied once)
    std::vector<TextEdit> skipped; ///< edits overlapping an earlier one, left for the next run
    std::string diff;             ///< unified diff from original to updated ("" when unchanged)
};

/// Applies non-overlapping edits to `original`. Identical edits collapse to
/// one; an edit that overlaps one already taken (in file order) is skipped,
/// so a second --fix run can pick it up against the rewritten text.
/// `displayPath` names the file in the diff headers.
FilePlan planEdits(const std::string& original, std::vector<TextEdit> edits, const std::string& displayPath);

} // namespace uvmcheck::fix
