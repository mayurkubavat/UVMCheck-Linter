#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::oop {

class DowncastVisitor : public slang::syntax::SyntaxVisitor<DowncastVisitor> {
public:
    explicit DowncastVisitor(core::DiagnosticEngine& diags, const uvmcheck::uvm::UvmModel& uvmModel)
        : diags(diags) {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (cls) {
                knownClasses.insert(std::string(cls->name));
            }
        }
    }

    void handle(const slang::syntax::ClassDeclarationSyntax& clsNode) {
        if (!clsNode.name.valueText().empty()) {
            knownClasses.insert(std::string(clsNode.name.valueText()));
        }
        visitDefault(clsNode);
    }

    void handle(const slang::syntax::CastExpressionSyntax& node) {
        if (node.left) {
            std::string typeName = std::string(node.left->toString());
            typeName.erase(0, typeName.find_first_not_of(" \t\n\r"));
            if (size_t end = typeName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                typeName.erase(end + 1);
            }

            bool isClassType = false;
            if (knownClasses.find(typeName) != knownClasses.end()) {
                isClassType = true;
            } else if (typeName.find("uvm_") != std::string::npos ||
                       typeName.find("_item") != std::string::npos ||
                       typeName.find("_seq") != std::string::npos ||
                       typeName.find("_driver") != std::string::npos ||
                       typeName.find("_monitor") != std::string::npos ||
                       typeName.find("_scoreboard") != std::string::npos ||
                       typeName.find("_env") != std::string::npos ||
                       typeName.find("_component") != std::string::npos ||
                       typeName.find("_object") != std::string::npos) {
                isClassType = true;
            }

            if (isClassType) {
                diags.addViolation(
                    "SV-OOP-011",
                    "DowncastWithoutDynamicCast",
                    "Explicit static cast '" + typeName + "'(...)' used on class handle. "
                    "In SystemVerilog (IEEE 1800 § 8.16), downcasting class handles requires dynamic casting via "
                    "'$cast(target, source)' to verify polymorphic type compatibility at runtime. "
                    "Static class casting bypasses runtime type safety and causes simulator aborts. Replace with '$cast()'.",
                    core::Severity::Error,
                    node.sourceRange().start()
                );
            }
        }
        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
    std::unordered_set<std::string> knownClasses;
};

class DowncastWithoutDynamicCast : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-011"; }
    std::string_view getName() const override { return "DowncastWithoutDynamicCast"; }
    std::string_view getDescription() const override {
        return "Flags static casting syntax (e.g. DerivedClass'(base_handle)) applied to class objects. "
               "SystemVerilog requires dynamic casting via $cast() to verify runtime polymorphism.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        DowncastVisitor visitor(diags, uvmModel);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDowncastWithoutDynamicCast() {
    return std::make_unique<oop::DowncastWithoutDynamicCast>();
}
}
