#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::uvm {

class ComponentNameMismatchVisitor : public slang::syntax::SyntaxVisitor<ComponentNameMismatchVisitor> {
public:
    explicit ComponentNameMismatchVisitor(core::DiagnosticEngine& diags, const uvmcheck::uvm::UvmModel& uvmModel)
        : diags(diags), uvmModel(uvmModel) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& clsNode) {
        std::string prevClass = currentClass;
        bool prevIsComp = currentIsComp;

        currentClass = clsNode.name.valueText().empty() ? "" : std::string(clsNode.name.valueText());
        currentIsComp = false;

        if (!currentClass.empty() && currentClass != "uvm_component") {
            for (const auto* cls : uvmModel.getAllClasses()) {
                if (cls && cls->name == currentClass) {
                    if (uvmModel.isUvmComponent(*cls)) {
                        currentIsComp = true;
                    }
                    break;
                }
            }

            if (!currentIsComp && clsNode.extendsClause && clsNode.extendsClause->baseName) {
                std::string baseName = std::string(clsNode.extendsClause->baseName->toString());
                if (baseName.find("uvm_component") != std::string::npos ||
                    baseName.find("uvm_env") != std::string::npos ||
                    baseName.find("uvm_agent") != std::string::npos ||
                    baseName.find("uvm_driver") != std::string::npos ||
                    baseName.find("uvm_monitor") != std::string::npos ||
                    baseName.find("uvm_scoreboard") != std::string::npos ||
                    baseName.find("uvm_test") != std::string::npos) {
                    currentIsComp = true;
                }
            }
        }

        visitDefault(clsNode);

        currentClass = prevClass;
        currentIsComp = prevIsComp;
    }

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (currentIsComp && func.prototype && func.prototype->name) {
            auto nameTok = func.prototype->name->getLastToken();
            if (nameTok.kind == slang::parsing::TokenKind::NewKeyword || nameTok.valueText() == "new") {
                checkConstructor(func);
            }
        }
        visitDefault(func);
    }

private:
    void checkConstructor(const slang::syntax::FunctionDeclarationSyntax& func) {
        std::string paramName = "name";
        if (func.prototype && func.prototype->portList) {
            for (const auto* port : func.prototype->portList->ports) {
                if (port && port->kind == slang::syntax::SyntaxKind::FunctionPort) {
                    const auto& fp = port->as<slang::syntax::FunctionPortSyntax>();
                    if (fp.declarator) {
                        paramName = std::string(fp.declarator->name.valueText());
                        break;
                    }
                }
            }
        }

        class SuperNewVisitor : public slang::syntax::SyntaxVisitor<SuperNewVisitor> {
        public:
            const slang::syntax::ArgumentListSyntax* argList = nullptr;

            void handle(const slang::syntax::NewClassExpressionSyntax& expr) {
                if (expr.scopedNew && expr.scopedNew->toString().find("super.new") != std::string::npos) {
                    argList = expr.argList;
                }
                visitDefault(expr);
            }

            void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
                if (inv.left && inv.left->toString().find("super.new") != std::string::npos) {
                    argList = inv.arguments;
                }
                visitDefault(inv);
            }
        };

        SuperNewVisitor snVisitor;
        func.visit(snVisitor);

        if (snVisitor.argList) {
            const auto& args = snVisitor.argList->parameters;
            if (args.size() >= 1 && args[0]) {
                const auto* firstArg = args[0];
                if (firstArg->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                    const auto& ordArg = firstArg->as<slang::syntax::OrderedArgumentSyntax>();
                    if (ordArg.expr) {
                        std::string argText = std::string(ordArg.expr->toString());
                        argText.erase(0, argText.find_first_not_of(" \t\n\r"));
                        if (size_t end = argText.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                            argText.erase(end + 1);
                        }

                        // Check if argument is a string literal: e.g. "my_comp"
                        if (argText.size() >= 2 && argText.front() == '"' && argText.back() == '"') {
                            std::vector<core::FixItHint> fixIts;
                            if (ordArg.expr->sourceRange().start().valid()) {
                                fixIts.push_back({
                                    ordArg.expr->sourceRange(),
                                    paramName,
                                    "Replace hardcoded literal with constructor parameter '" + paramName + "'"
                                });
                            }

                            diags.addViolation(
                                "UVM-STY-003",
                                "ComponentNameMismatch",
                                "Constructor of UVM component '" + currentClass +
                                "' passes hardcoded string literal " + argText +
                                " to super.new(...) instead of forwarding parameter '" + paramName +
                                "'. Hardcoded component names override factory instance names and break hierarchical config_db resolution.",
                                core::Severity::Error,
                                ordArg.expr->sourceRange().start(),
                                std::move(fixIts)
                            );
                        }
                    }
                }
            }
        }
    }

    core::DiagnosticEngine& diags;
    const uvmcheck::uvm::UvmModel& uvmModel;
    std::string currentClass;
    bool currentIsComp = false;
};

class ComponentNameMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-STY-003"; }
    std::string_view getName() const override { return "ComponentNameMismatch"; }
    std::string_view getDescription() const override {
        return "Disallows passing hardcoded string literals to super.new() in uvm_component constructors. "
               "The constructor must forward the 'name' argument so the UVM factory can assign distinct instance names.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        ComponentNameMismatchVisitor visitor(diags, uvmModel);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createComponentNameMismatch() {
    return std::make_unique<uvm::ComponentNameMismatch>();
}
}
