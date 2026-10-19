#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include <unordered_map>

namespace uvmcheck::rules::portability {

class ProprietaryVendorCallsVisitor : public slang::syntax::SyntaxVisitor<ProprietaryVendorCallsVisitor> {
public:
    explicit ProprietaryVendorCallsVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::SystemNameSyntax& sysName) {
        std::string subName = std::string(sysName.systemIdentifier.valueText());
        if (subName.starts_with('$')) subName = subName.substr(1);
        static const std::unordered_map<std::string, std::string> vendorCalls = {
            {"vcdpluson", "simulator-specific"},
            {"vcdplusoff", "simulator-specific"},
            {"vcdplusmemon", "simulator-specific"},
            {"vcdplusmemoff", "simulator-specific"},
            {"shm_open", "simulator-specific"},
            {"shm_close", "simulator-specific"},
            {"shm_probe", "simulator-specific"},
            {"fsdbDumpvars", "simulator-specific"},
            {"fsdbDumpfile", "simulator-specific"},
            {"fsdbDumpflush", "simulator-specific"},
            {"wlfdump", "simulator-specific"},
            {"wlfdumpvars", "simulator-specific"}
        };

        auto it = vendorCalls.find(subName);
        if (it != vendorCalls.end()) {
            diags.addViolation(
                "PORT-PRP-001",
                "ProprietaryVendorCalls",
                "Proprietary " + it->second + " system task '$" + subName + "' detected. "
                "Vendor-proprietary simulation tasks break VIP portability across simulators. "
                "Wrap proprietary calls in a simulator-specific `ifdef guard or use standard IEEE 1800 dump tasks ($dumpvars, $dumpfile).",
                core::Severity::Warning,
                sysName.sourceRange().start()
            );
        }

        visitDefault(sysName);
    }

private:
    core::DiagnosticEngine& diags;
};

class ProprietaryVendorCalls : public Rule {
public:
    std::string_view getId() const override { return "PORT-PRP-001"; }
    std::string_view getName() const override { return "ProprietaryVendorCalls"; }
    std::string_view getDescription() const override {
        return "Disallow unguarded simulator-proprietary system tasks ($vcdpluson, $fsdbDumpvars, $shm_open). "
               "Vendor-specific calls prevent Verification IP from running on other simulators.";
    }
    std::string_view getCategory() const override { return "Portability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ProprietaryVendorCallsVisitor visitor(diags);
        for (const auto& syntaxTree : compilation.getSyntaxTrees()) {
            if (syntaxTree) {
                syntaxTree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::portability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProprietaryVendorCalls() {
    return std::make_unique<portability::ProprietaryVendorCalls>();
}
}
