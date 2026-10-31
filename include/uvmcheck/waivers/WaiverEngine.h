#pragma once

#include <filesystem>
#include <optional>
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

    /// Root that baseline paths are relative to (the folder holding .uvmcheck/, or the
    /// working directory). Defaults to the working directory.
    void setProjectRoot(std::filesystem::path root) { projectRoot = std::move(root); }

    /// Load external waiver rules from a JSON file: a list of {"rule", "path", "line",
    /// "reason"} objects, or an object with such a list under "waivers".
    bool loadWaivers(const std::string& waiverFilePath);

    /// Add a waiver declared in the project config
    void addWaiver(ExternalWaiver waiver) { externalWaivers.push_back(std::move(waiver)); }

    /// Load baseline entries from a JSON file
    bool loadBaseline(const std::string& baselinePath);

    /// Save violations to a baseline JSON file
    bool saveBaseline(const std::string& baselinePath, const std::vector<core::Violation>& violations);

    /// Filter a vector of violations, returning only active non-waived violations
    std::vector<core::Violation> filter(const std::vector<core::Violation>& violations);

    size_t getWaivedCount() const { return waivedCount; }

    /// Baseline entries that matched no finding in the last filter() call: fixed findings,
    /// or findings whose line changed. --update-baseline drops them.
    size_t getStaleBaselineCount() const;

    /// Fingerprint of each violation, in the same order (exposed for testing).
    std::vector<std::string> fingerprints(const std::vector<core::Violation>& violations);

    /// Path of a violation's file as stored in the baseline.
    std::string baselinePath(std::string_view filePath) const;

    /// Last error from loadWaivers/loadBaseline (parse errors).
    const std::string& getLastError() const { return lastError; }

private:
    [[maybe_unused]] const slang::SourceManager& sourceManager;
    std::filesystem::path projectRoot = std::filesystem::current_path();
    std::unordered_set<std::string> baselineFingerprints;
    std::unordered_set<std::string> legacyFingerprints;
    std::unordered_set<std::string> matchedFingerprints;
    std::vector<ExternalWaiver> externalWaivers;
    std::unordered_map<std::string, std::vector<std::string>> fileLines;
    size_t waivedCount = 0;
    std::string lastError;

    std::string_view lineText(const core::Violation& v, size_t line);
    std::string legacyFingerprint(const core::Violation& v) const;
    bool hasInlineWaiver(const core::Violation& v);
    bool matchesExternalWaiver(const core::Violation& v) const;
};

} // namespace uvmcheck::waivers
