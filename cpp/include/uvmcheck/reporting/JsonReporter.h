#pragma once

#include <memory>
#include "uvmcheck/rules/Rule.h"

#include <string>
#include <vector>
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/reporting/Conformance.h"

namespace uvmcheck::reporting {

/// Machine-readable findings report, meant for test harnesses and CI scripts
/// that need exact rule IDs and locations without scraping console output.
class JsonReporter {
public:
    /// Writes the report to outputPath, or to stdout when outputPath is "-". Each finding
    /// lists the methodology guidelines its rule enforces; `conformance` adds a per-guideline
    /// pass/fail section; `overrides` lists the slang overlay overrides that were active.
    static bool writeJson(const std::string& outputPath,
                          const std::vector<core::Violation>& violations,
                          size_t compileErrors,
                          size_t waivedCount,
                          const std::vector<GuidelineResult>& conformance = {},
                          const std::vector<std::string>& fingerprints = {},
                          const std::vector<std::string>& overrides = {});

    /// The guide itself: every guideline with its level, statement and rules.
    static bool writeGuidelines(const std::string& outputPath);

    /// The rule catalog: every registered rule and sub-rule with its metadata, sorted by ID.
    /// docs/rules/catalog.json is generated from this (scripts/gen_rule_docs.py).
    static bool writeRuleCatalog(const std::string& outputPath,
                                 const std::vector<std::unique_ptr<rules::Rule>>& rules);
};

} // namespace uvmcheck::reporting
