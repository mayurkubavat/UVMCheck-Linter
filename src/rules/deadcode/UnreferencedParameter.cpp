#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/ast/Scope.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include <unordered_map>
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::deadcode {

class ParamDeclVisitor : public slang::ast::ASTVisitor<ParamDeclVisitor, slang::ast::VisitFlags::AllGood> {
public:
    struct ParamInfo {
        slang::SourceLocation loc;
        std::string scopeName;
    };

    void handle(const slang::ast::ParameterSymbol& param) {
        if (!param.name.empty() && !param.name.starts_with("$")) {
            std::string pName = std::string(param.name);
            std::string sName = param.getParentScope() ? std::string(param.getParentScope()->asSymbol().name) : "global";
            if (sName.empty()) sName = "global";
            declaredParams[pName] = { param.location, sName };
        }
        visitDefault(param);
    }

    void handle(const slang::ast::NamedValueExpression& expr) {
        referencedParams.insert(std::string(expr.symbol.name));
        visitDefault(expr);
    }

    std::unordered_map<std::string, ParamInfo> declaredParams;
    std::unordered_set<std::string> referencedParams;
};

class UnreferencedParameter : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-012"; }
    std::string_view getName() const override { return "UnreferencedParameter"; }
    std::string_view getDescription() const override {
        return "Flags parameter and localparam declarations that are never referenced within their scope or testbench. "
               "Unused parameters create clutter and indicate dead or abandoned configurations.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ParamDeclVisitor visitor;
        compilation.getRoot().visit(visitor);

        for (const auto& [name, info] : visitor.declaredParams) {
            if (visitor.referencedParams.find(name) == visitor.referencedParams.end()) {
                // Avoid flagging standard UVM or simulator built-ins
                if (name == "UVM_NONE" || name == "UVM_LOW" || name == "UVM_MEDIUM" ||
                    name == "UVM_HIGH" || name == "UVM_FULL" || name == "UVM_DEBUG") {
                    continue;
                }

                diags.addViolation(
                    "SV-DED-012",
                    "UnreferencedParameter",
                    "Parameter '" + name + "' is declared in scope '" + info.scopeName + "' but never referenced. "
                    "Unreferenced parameters complicate module and VIP interfaces, and often indicate forgotten "
                    "configuration logic or obsolete constants.",
                    core::Severity::Warning,
                    info.loc
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnreferencedParameter() {
    return std::make_unique<deadcode::UnreferencedParameter>();
}
}
