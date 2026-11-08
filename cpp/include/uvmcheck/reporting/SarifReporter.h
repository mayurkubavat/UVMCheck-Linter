#pragma once

#include <string>
#include <vector>
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/rules/RuleRegistry.h"

namespace uvmcheck::reporting {

class SarifReporter {
public:
    static bool writeSarif(const std::string& outputPath,
                           const std::vector<core::Violation>& violations,
                           const std::vector<std::unique_ptr<rules::Rule>>& rules);
};

} // namespace uvmcheck::reporting
