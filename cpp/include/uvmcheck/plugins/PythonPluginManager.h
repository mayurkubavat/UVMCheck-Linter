#pragma once

#include <ostream>
#include <string>

#include "slang/ast/Compilation.h"
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::plugins {

/// Runs Python rules (subclasses of uvmcheck.Rule) against an exported design model.
///
/// The model (version 2, see python/uvmcheck/context.py) holds the user classes with their
/// role, base chain, properties and methods; each method's arguments and the calls it makes
/// (name, receiver, declaring class, argument text); config_db set/get calls; TLM ports and
/// connect() calls; and the per-rule options from the project config. It is written to a
/// private temporary file, the runner is started without a shell, and its JSON findings are
/// read from stdout (its stderr passes through).
class PythonPluginManager {
public:
    static bool runPythonRules(const std::string& rulesPath,
                               slang::ast::Compilation& compilation,
                               const uvmcheck::uvm::UvmModel& uvmModel,
                               core::DiagnosticEngine& diags);

    /// Writes the model the Python rules see (exposed for testing and --dump-model use).
    static void writeModel(std::ostream& out, slang::ast::Compilation& compilation,
                           const uvmcheck::uvm::UvmModel& uvmModel);
};

} // namespace uvmcheck::plugins
