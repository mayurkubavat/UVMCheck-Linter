#pragma once

#include <filesystem>
#include <memory>
#include <optional>
#include <regex>
#include <string>
#include <string_view>
#include <unordered_map>
#include <unordered_set>
#include <vector>
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/core/ProjectConfig.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::waivers {

struct ExternalWaiver {
    std::string ruleId;      // e.g. "SV-PRF-001" or "*"
    std::string pathPattern; // e.g. "*sample_tb.sv" or substring
    size_t line = 0;         // 0 means any line
    std::string reason;
    std::optional<core::PathPattern> glob; // project-config waivers match by glob instead
    std::string owner;
    std::string label;  // how the hygiene report names it, e.g. "waivers.json #3"
    size_t group = 0;   // entries expanded from one written waiver share a group
    /// Optional ECMAScript regex. Empty matches any message. Compiled once at load.
    std::string messagePattern;
    std::shared_ptr<const std::regex> messageRegex;
    /// Set: keep the finding and report it at this severity instead of waiving it.
    std::optional<core::Severity> severity;
};

/// An inline message regex that did not compile.
struct MessageRegexError {
    std::string filePath;
    size_t line = 0;
    std::string pattern;
    std::string what;
};

/// A waiver comment in a source file (`// uvmcheck:waive RULE` and friends).
struct InlineWaiver {
    std::string filePath;
    size_t line = 0;
    std::string text;
};

/// What --waiver-report and --strict-waivers look at after filter().
struct WaiverHygiene {
    size_t written = 0;                       // config + waiver-file waivers, as written
    std::vector<std::string> unused;          // labels of written waivers that matched nothing
    size_t withoutOwner = 0;
    std::vector<std::pair<std::string, size_t>> byRule, byOwner; // findings waived
    size_t inlineComments = 0;
    std::vector<InlineWaiver> unusedInline;
    size_t baselineEntries = 0, staleBaseline = 0;
};

/// Suppresses findings through inline comments, external/config waivers and a baseline.
///
/// Baseline entries are matched by a fingerprint that survives edits elsewhere in the file:
/// a hash of the rule ID, the file's path relative to the project root, the text of the
/// finding's line with whitespace collapsed, and the occurrence index among findings with
/// the same rule, file and line text. Inserting or deleting lines above a finding, or
/// moving the checkout, keeps the fingerprint; changing the flagged line itself does not.
/// Version 1.0 baselines (rule@basename:line) are still read.
class WaiverEngine {
public:
    explicit WaiverEngine(const slang::SourceManager& sm);

    /// Supplies a file's current text (e.g. an unsaved editor buffer) for
    /// inline waiver lookups, instead of reading it from disk.
    void setFileText(const std::string& filePath, std::string_view text);

    /// Root that baseline paths are relative to (the folder holding .uvmcheck/, or the
    /// working directory). Defaults to the working directory.
    void setProjectRoot(std::filesystem::path root) { projectRoot = std::move(root); }

    /// Load external waiver rules from a JSON file: a list of {"rule", "path", "line",
    /// "reason", "message"} objects, or an object with such a list under "waivers".
    /// `message`, when set, is an ECMAScript regex searched against the diagnostic
    /// text. An invalid pattern fails the load; the error names this file and the waiver.
    bool loadWaivers(const std::string& waiverFilePath);

    /// Add a waiver declared in the project config
    void addWaiver(ExternalWaiver waiver);

    /// Load baseline entries from a JSON file
    bool loadBaseline(const std::string& baselinePath);

    /// Save violations to a baseline JSON file
    bool saveBaseline(const std::string& baselinePath, const std::vector<core::Violation>& violations);

    /// Filter a vector of violations, returning only active non-waived violations
    std::vector<core::Violation> filter(const std::vector<core::Violation>& violations);

    size_t getWaivedCount() const { return waivedCount; }
    /// Findings kept at another severity by a waiver with a severity (last filter()).
    size_t getRegradedCount() const { return regradedCount; }

    /// Baseline entries that matched no finding in the last filter() call: fixed findings,
    /// or findings whose line changed. --update-baseline drops them.
    size_t getStaleBaselineCount() const;

    /// Usage of every waiver in the last filter() call. `files` are the user
    /// files to scan for waiver comments; a comment counts as unused only if it
    /// names a rule in `activeRules` (or '*') and waived nothing.
    WaiverHygiene hygiene(const std::vector<std::string>& files,
                          const std::unordered_set<std::string>& activeRules);

    /// Fingerprint of each violation, in the same order (exposed for testing).
    std::vector<std::string> fingerprints(const std::vector<core::Violation>& violations);

    /// Path of a violation's file as stored in the baseline.
    std::string baselinePath(std::string_view filePath) const;

    /// Last error from loadWaivers/loadBaseline (parse errors).
    const std::string& getLastError() const { return lastError; }

    /// Inline `message` patterns that failed to compile. Config and waiver-file
    /// patterns fail the load instead of landing here.
    const std::vector<MessageRegexError>& regexErrors() const { return badRegexes; }

private:
    [[maybe_unused]] const slang::SourceManager& sourceManager;
    std::filesystem::path projectRoot = std::filesystem::current_path();
    std::unordered_set<std::string> baselineFingerprints;
    std::unordered_set<std::string> legacyFingerprints;
    std::unordered_set<std::string> matchedFingerprints;
    std::vector<ExternalWaiver> externalWaivers;
    std::unordered_map<std::string, std::shared_ptr<const std::regex>> compiledMessages;
    std::vector<MessageRegexError> badRegexes;
    std::vector<size_t> groupMatches;                       // findings matched per waiver group
    std::unordered_set<std::string> usedInline;              // "file\nline" of comments that waived something
    std::unordered_map<std::string, size_t> waivedByRule, waivedByOwner;
    std::unordered_map<std::string, std::vector<std::string>> fileLines;
    size_t waivedCount = 0;
    size_t regradedCount = 0;
    std::string lastError;

    std::string_view lineText(const core::Violation& v, size_t line);
    const std::vector<std::string>& fileLinesOf(const std::string& path);
    /// Compiles `pattern` once. Returns nullptr and records an error when it is invalid.
    const std::regex* messageRegexFor(const std::string& pattern, const std::string& file, size_t line);
    bool commentWaives(std::string_view text, const core::Violation& v, size_t commentLine);
    std::string legacyFingerprint(const core::Violation& v) const;
    bool hasInlineWaiver(const core::Violation& v);
    const ExternalWaiver* matchExternalWaiver(const core::Violation& v);
};

} // namespace uvmcheck::waivers
