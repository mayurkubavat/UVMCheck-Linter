#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/ConfigDbGraph.h"

#include <string>

namespace uvmcheck::rules::uvm {

class ConfigDbCrossComponentResolver : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-005"; }
    std::string_view getName() const override { return "ConfigDbCrossComponentResolver"; }
    std::string_view getDescription() const override {
        return "Performs cross-component semantic resolution on uvm_config_db calls across the testbench "
               "hierarchy DAG to detect configuration type mismatches between set() and get() calls.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        uvmcheck::uvm::ConfigDbGraph graph(compilation);
        auto mismatches = graph.findMismatches();

        // A type declared in the calling class (`CFG_T`, a type parameter of
        // dv_base_agent) depends on the specialization: the pair cannot be told
        // apart. The native rule skips the same pairs.
        auto classScoped = [](const std::string& ty, const std::string& cls) {
            return !cls.empty() && (ty.rfind(cls + "::", 0) == 0 || ty.find("::" + cls + "::") != std::string::npos);
        };
        for (const auto& m : mismatches) {
            if (classScoped(m.getType, m.getClass) || classScoped(m.setType, m.setClass)) continue;
            diags.addViolation(
                "UVM-CFG-005",
                "ConfigDbCrossComponentResolver",
                "Cross-component 'uvm_config_db' type mismatch for field '" + m.fieldName + "': " +
                "configured in class '" + m.setClass + "' with type '" + m.setType + "', but " +
                "retrieved in class '" + m.getClass + "' with incompatible type '" + m.getType + "'. " +
                "Mismatched type parameters break runtime config lookup and leave fields uninitialized.",
                core::Severity::Error,
                m.getLocation
            );
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConfigDbCrossComponentResolver() {
    return std::make_unique<uvm::ConfigDbCrossComponentResolver>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createConfigDbCrossComponentResolver)
