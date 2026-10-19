#pragma once

#include <string>
#include <string_view>
#include <unordered_set>
#include <vector>
#include <optional>
#include "uvmcheck/core/Diagnostic.h"
#include "slang/text/SourceManager.h"

namespace uvmcheck::waivers {

struct ExternalWaiver {
    std::string ruleId;      // e.g. "SV-PRF-001" or "*"
    std::string pathPattern; // e.g. "*sample_tb.sv" or substring
    size_t line = 0;         // 0 means any line
    std::string reason;
};

class WaiverEngine {
public:
    explicit WaiverEngine(const slang::SourceManager& sm);

    /// Load external waiver rules from a JSON file
    bool loadWaivers(const std::string& waiverFilePath);

    /// Load baseline fingerprints from a JSON file
    bool loadBaseline(const std::string& baselinePath);

    /// Save violations to a baseline JSON file
    bool saveBaseline(const std::string& baselinePath, const std::vector<core::Violation>& violations);

    /// Checks if a violation is waived either by inline comment, external waiver, or baseline snapshot
    bool isWaived(const core::Violation& v) const;

    /// Filter a vector of violations, returning only active non-waived violations
    std::vector<core::Violation> filter(const std::vector<core::Violation>& violations) const;

    size_t getWaivedCount() const { return waivedCount; }

private:
    [[maybe_unused]] const slang::SourceManager& sourceManager;
    std::unordered_set<std::string> baselineFingerprints;
    std::vector<ExternalWaiver> externalWaivers;
    mutable size_t waivedCount = 0;

    std::string computeFingerprint(const core::Violation& v) const;
    bool hasInlineWaiver(const core::Violation& v) const;
    bool matchesExternalWaiver(const core::Violation& v) const;
};

} // namespace uvmcheck::waivers
