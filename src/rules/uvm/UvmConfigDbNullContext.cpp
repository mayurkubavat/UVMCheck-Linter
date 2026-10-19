#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::uvm {

class ConfigDbNullVisitor : public slang::ast::ASTVisitor<ConfigDbNullVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit ConfigDbNullVisitor(std::string_view compName, core::DiagnosticEngine& diags)
        : componentName(compName), diags(diags) {}

    void handle(const slang::ast::CallExpression& call) {
        if (call.subroutine.index() == 0) {
            const auto* sub = std::get<const slang::ast::SubroutineSymbol*>(call.subroutine);
            if (sub && (sub->name == "get" || sub->name == "set")) {
                const auto* parentScope = sub->getParentScope();
                if (parentScope) {
                    std::string_view parentName = parentScope->asSymbol().name;
                    if (parentScope->asSymbol().kind == slang::ast::SymbolKind::ClassType) {
                        const auto& ct = parentScope->asSymbol().as<slang::ast::ClassType>();
                        if (ct.genericClass) parentName = ct.genericClass->name;
                        else parentName = ct.name;
                    }
                    if (parentName.find("uvm_config_db") != std::string_view::npos) {
                        auto args = call.arguments();
                        if (!args.empty() && args[0]->unwrapImplicitConversions().kind == slang::ast::ExpressionKind::NullLiteral) {
                            diags.addViolation(
                                "UVM-CFG-002",
                                "UvmConfigDbNullContext",
                                "Passing 'null' as context to uvm_config_db::" + std::string(sub->name) +
                                " inside uvm_component '" + std::string(componentName) +
                                "'. Pass 'this' instead to preserve hierarchical scoping.",
                                core::Severity::Warning,
                                args[0]->sourceRange.start()
                            );
                        }
                    }
                }
            }
        }
        visitDefault(call);
    }

private:
    std::string_view componentName;
    core::DiagnosticEngine& diags;
};

class UvmConfigDbNullContext : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-002"; }
    std::string_view getName() const override { return "UvmConfigDbNullContext"; }
    std::string_view getDescription() const override {
        return "Passing 'null' as context to uvm_config_db inside a uvm_component breaks hierarchical "
               "scoping and prevents targeted configuration overrides. Use 'this' instead.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            if (!uvmModel.isUvmComponent(*classType)) continue;

            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                ConfigDbNullVisitor visitor(classType->name, diags);
                sub.getBody().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmConfigDbNullContext() {
    return std::make_unique<uvm::UvmConfigDbNullContext>();
}
}
