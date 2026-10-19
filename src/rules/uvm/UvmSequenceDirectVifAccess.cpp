#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/types/Type.h"

namespace uvmcheck::rules::uvm {

class UvmSequenceDirectVifAccess : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-001"; }
    std::string_view getName() const override { return "UvmSequenceDirectVifAccess"; }
    std::string_view getDescription() const override {
        return "Sequences and sequence items must not directly declare or drive virtual interface signals. "
               "Signal-level driving is the sole responsibility of the driver.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            bool isSeq = uvmModel.isUvmSequence(*cls);
            bool isSeqItem = uvmModel.isUvmSequenceItem(*cls);

            if (!isSeq && !isSeqItem) continue;

            // 1. Check class property members
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    if (prop.getType().isVirtualInterfaceOrArray() || prop.getType().isVirtualInterface()) {
                        std::string kindStr = isSeq ? "Sequence" : "Sequence item";
                        diags.addViolation(
                            "UVM-SEQ-001",
                            "UvmSequenceDirectVifAccess",
                            kindStr + " class '" + std::string(cls->name) +
                            "' directly declares virtual interface property '" + std::string(prop.name) +
                            "'. Sequences must remain protocol-abstract and transaction-level; "
                            "pin-level driving is the responsibility of the driver component.",
                            core::Severity::Warning,
                            prop.location
                        );
                    }
                }
                // 2. Check local variables inside methods
                else if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    for (const auto& localMember : sub.members()) {
                        if (localMember.kind == slang::ast::SymbolKind::Variable) {
                            const auto& var = localMember.as<slang::ast::VariableSymbol>();
                            if (var.getType().isVirtualInterfaceOrArray() || var.getType().isVirtualInterface()) {
                                diags.addViolation(
                                    "UVM-SEQ-001",
                                    "UvmSequenceDirectVifAccess",
                                    "Method '" + std::string(sub.name) + "' in sequence class '" +
                                    std::string(cls->name) + "' declares virtual interface variable '" +
                                    std::string(var.name) + "'. Pin-level driving must be confined to drivers.",
                                    core::Severity::Warning,
                                    var.location
                                );
                            }
                        }
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmSequenceDirectVifAccess() {
    return std::make_unique<uvm::UvmSequenceDirectVifAccess>();
}
}
