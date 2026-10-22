#pragma once

#include <string>
#include <vector>
#include "uvmcheck/core/Diagnostic.h"

namespace uvmcheck::reporting {

/// Machine-readable findings report, meant for test harnesses and CI scripts
/// that need exact rule IDs and locations without scraping console output.
class JsonReporter {
public:
    /// Writes the report to outputPath, or to stdout when outputPath is "-".
    static bool writeJson(const std::string& outputPath,
                          const std::vector<core::Violation>& violations,
                          size_t compileErrors,
                          size_t waivedCount);
};

} // namespace uvmcheck::reporting
