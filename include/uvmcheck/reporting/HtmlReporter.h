#pragma once

#include <string>
#include <vector>
#include <memory>
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/rules/Rule.h"

namespace uvmcheck::reporting {

class HtmlReporter {
public:
    /// Generates a standalone, dependency-free interactive HTML dashboard
    static bool writeHtml(const std::string& outputPath,
                          const std::vector<core::Violation>& violations,
                          const std::vector<std::unique_ptr<rules::Rule>>& rules);

private:
    static std::string escapeHtml(std::string_view str);
};

} // namespace uvmcheck::reporting
