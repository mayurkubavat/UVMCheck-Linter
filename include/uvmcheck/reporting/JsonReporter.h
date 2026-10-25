#pragma once

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
    /// pass/fail section.
    static bool writeJson(const std::string& outputPath,
                          const std::vector<core::Violation>& violations,
                          size_t compileErrors,
                          size_t waivedCount,
                          const std::vector<GuidelineResult>& conformance = {});

    /// The guide itself: every guideline with its level, statement and rules.
    static bool writeGuidelines(const std::string& outputPath);
};

} // namespace uvmcheck::reporting
