#pragma once

#include <filesystem>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

namespace uvmcheck::core {

/// Lines added or modified since a git revision, from `git diff -U0 <ref>`
/// plus untracked files (treated as entirely new). Paths are canonical.
class ChangedLines {
public:
    /// Reads the changes of the repository holding `dir`. Returns false (see
    /// error()) when there is no repository or the revision is unknown.
    bool load(const std::string& ref, const std::filesystem::path& dir);

    bool contains(const std::string& filePath, size_t line) const;
    /// A file that does not exist at the ref: added in the diff, or untracked.
    bool isNewFile(const std::string& filePath) const;
    const std::string& error() const { return lastError; }
    size_t fileCount() const { return ranges.size() + wholeFiles.size(); }

    /// Parses unified diff text with zero context (exposed for testing).
    void parseDiff(const std::string& diff, const std::filesystem::path& root);

private:
    std::unordered_map<std::string, std::vector<std::pair<size_t, size_t>>> ranges; // [first, last]
    std::unordered_set<std::string> wholeFiles;
    std::unordered_set<std::string> newFiles;
    std::string lastError;
};

} // namespace uvmcheck::core
