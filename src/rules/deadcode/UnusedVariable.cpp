#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include <unordered_set>

namespace uvmcheck::rules::deadcode {

class VariableReferenceVisitor : public slang::ast::ASTVisitor<VariableReferenceVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::NamedValueExpression& expr) {
        referencedSymbols.insert(&expr.symbol);
        visitDefault(expr);
    }

    std::unordered_set<const slang::ast::Symbol*> referencedSymbols;
};

class UnusedVariable : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-001"; }
    std::string_view getName() const override { return "UnusedVariable"; }
    std::string_view getDescription() const override {
        return "Local variables declared in tasks or functions should be referenced. "
               "Unused variables clutter code, consume memory, and often indicate incomplete logic.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            for (const auto& member : cls->members()) {
                if (member.kind != slang::ast::SymbolKind::Subroutine) continue;

                const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                // Collect local variables declared inside this subroutine
                std::vector<const slang::ast::VariableSymbol*> localVars;
                for (const auto& subMember : sub.members()) {
                    if (subMember.kind == slang::ast::SymbolKind::Variable) {
                        const auto& var = subMember.as<slang::ast::VariableSymbol>();
                        // Skip compiler-generated variables
                        if (var.flags.has(slang::ast::VariableFlags::CompilerGenerated)) continue;
                        localVars.push_back(&var);
                    }
                }

                if (localVars.empty()) continue;

                // Visit subroutine body to collect all referenced symbols
                VariableReferenceVisitor refVisitor;
                sub.getBody().visit(refVisitor);

                for (const auto* var : localVars) {
                    if (refVisitor.referencedSymbols.find(var) == refVisitor.referencedSymbols.end()) {
                        diags.addViolation(
                            "SV-DED-001",
                            "UnusedVariable",
                            "Local variable '" + std::string(var->name) + "' in method '" +
                            std::string(sub.name) + "' is declared but never referenced.",
                            core::Severity::Info,
                            var->location
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnusedVariable() {
    return std::make_unique<deadcode::UnusedVariable>();
}
}
