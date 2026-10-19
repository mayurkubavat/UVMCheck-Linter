#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::rules::performance {

class PassArrayByValue : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-001"; }
    std::string_view getName() const override { return "PassArrayByValue"; }
    std::string_view getDescription() const override {
        return "Passing dynamic arrays, queues, associative arrays, unpacked arrays, or unpacked structs "
               "by value (input) causes expensive memory duplication on every call. Use 'const ref' or 'ref' instead.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                for (const auto* arg : sub.getArguments()) {
                    if (arg->direction == slang::ast::ArgumentDirection::In) {
                        const auto& type = arg->getType().getCanonicalType();
                        auto kind = type.kind;

                        bool isLarge = (kind == slang::ast::SymbolKind::DynamicArrayType ||
                                        kind == slang::ast::SymbolKind::QueueType ||
                                        kind == slang::ast::SymbolKind::AssociativeArrayType ||
                                        kind == slang::ast::SymbolKind::FixedSizeUnpackedArrayType ||
                                        kind == slang::ast::SymbolKind::UnpackedStructType);

                        if (isLarge) {
                            diags.addViolation(
                                "SV-PRF-001",
                                "PassArrayByValue",
                                "Argument '" + std::string(arg->name) + "' of type '" +
                                std::string(type.toString()) + "' in method '" +
                                std::string(sub.name) + "' is passed by value (input). Use 'const ref' instead.",
                                core::Severity::Warning,
                                arg->location
                            );
                        }
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPassArrayByValue() {
    return std::make_unique<performance::PassArrayByValue>();
}
}
