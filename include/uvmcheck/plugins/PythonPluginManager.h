#pragma once

#include <string>
#include <string_view>
#include <vector>
#include "uvmcheck/uvm/UvmModel.h"
#include "uvmcheck/core/Diagnostic.h"

namespace uvmcheck::plugins {

class PythonPluginManager {
public:
    static bool runPythonRules(
        const std::string& rulesPath,
        const uvmcheck::uvm::UvmModel& uvmModel,
        core::DiagnosticEngine& diags);
};

} // namespace uvmcheck::plugins
