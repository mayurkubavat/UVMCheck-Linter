#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::rules::uvm {

class UvmPhaseSignatureMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-003"; }
    std::string_view getName() const override { return "UvmPhaseSignatureMismatch"; }
    std::string_view getDescription() const override {
        return "Standard UVM phase methods must declare correct signatures (function void vs task, "
               "and a single argument of type uvm_phase).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmComponent(*cls)) continue;

            for (const auto& member : cls->members()) {
                if (member.kind != slang::ast::SymbolKind::Subroutine) continue;

                const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                std::string subName = std::string(sub.name);
                bool isFuncPhase = uvmcheck::uvm::UvmModel::isFunctionPhase(subName);
                bool isTskPhase = uvmcheck::uvm::UvmModel::isTaskPhase(subName);

                if (!isFuncPhase && !isTskPhase) continue;

                // Check Function Phase rules
                if (isFuncPhase) {
                    if (sub.subroutineKind != slang::ast::SubroutineKind::Function) {
                        diags.addViolation(
                            "UVM-PHS-003",
                            "UvmPhaseSignatureMismatch",
                            "UVM phase '" + subName + "' in class '" + std::string(cls->name) +
                            "' is declared as a 'task'. Standard UVM function phases must be declared as 'function void'.",
                            core::Severity::Error,
                            sub.location
                        );
                    } else if (!sub.getReturnType().isVoid()) {
                        diags.addViolation(
                            "UVM-PHS-003",
                            "UvmPhaseSignatureMismatch",
                            "UVM phase '" + subName + "' in class '" + std::string(cls->name) +
                            "' has return type '" + std::string(sub.getReturnType().toString()) +
                            "'. Standard UVM phases must return 'void'.",
                            core::Severity::Error,
                            sub.location
                        );
                    }
                }

                // Check Task Phase rules
                if (isTskPhase) {
                    if (sub.subroutineKind != slang::ast::SubroutineKind::Task) {
                        diags.addViolation(
                            "UVM-PHS-003",
                            "UvmPhaseSignatureMismatch",
                            "UVM phase '" + subName + "' in class '" + std::string(cls->name) +
                            "' is declared as a 'function'. Time-consuming UVM phases must be declared as 'task'.",
                            core::Severity::Error,
                            sub.location
                        );
                    }
                }

                // Check argument count and type
                auto args = sub.getArguments();
                if (args.size() != 1) {
                    diags.addViolation(
                        "UVM-PHS-003",
                        "UvmPhaseSignatureMismatch",
                        "UVM phase '" + subName + "' in class '" + std::string(cls->name) +
                        "' declares " + std::to_string(args.size()) +
                        " argument(s), but standard UVM phases must declare exactly one argument of type 'uvm_phase'.",
                        core::Severity::Error,
                        sub.location
                    );
                } else {
                    const auto* arg = args[0];
                    const auto& argType = arg->getType();
                    bool isPhaseType = uvmModel.isDerivedFrom(argType, "uvm_phase") ||
                                       argType.name == "uvm_phase" ||
                                       argType.toString().find("uvm_phase") != std::string::npos;
                    if (!isPhaseType) {
                        diags.addViolation(
                            "UVM-PHS-003",
                            "UvmPhaseSignatureMismatch",
                            "Argument '" + std::string(arg->name) + "' of UVM phase '" + subName +
                            "' in class '" + std::string(cls->name) + "' has type '" +
                            std::string(argType.toString()) + "', but must be of type 'uvm_phase'.",
                            core::Severity::Error,
                            arg->location
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmPhaseSignatureMismatch() {
    return std::make_unique<uvm::UvmPhaseSignatureMismatch>();
}
}
