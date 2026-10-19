#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::oop {

class EmptyVirtualMethodVisitor : public slang::syntax::SyntaxVisitor<EmptyVirtualMethodVisitor> {
public:
    explicit EmptyVirtualMethodVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::string className = std::string(cls.name.valueText());

        for (const auto* member : cls.items) {
            if (member && member->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
                const auto& method = member->as<slang::syntax::ClassMethodDeclarationSyntax>();
                bool isVirtual = false;
                bool isPure = false;

                for (const auto& q : method.qualifiers) {
                    if (q.kind == slang::parsing::TokenKind::VirtualKeyword) isVirtual = true;
                    if (q.kind == slang::parsing::TokenKind::PureKeyword) isPure = true;
                }

                if (method.declaration && method.declaration->prototype) {
                    for (const auto* s : method.declaration->prototype->specifiers) {
                        if (s && s->keyword.kind == slang::parsing::TokenKind::VirtualKeyword) isVirtual = true;
                        if (s && s->keyword.kind == slang::parsing::TokenKind::PureKeyword) isPure = true;
                    }

                    if (isVirtual && !isPure) {
                        if (method.declaration->end.kind != slang::parsing::TokenKind::Unknown &&
                            method.declaration->items.empty()) {
                            std::string methodName;
                            if (method.declaration->prototype->name) {
                                methodName = std::string(method.declaration->prototype->name->getLastToken().valueText());
                            }

                            if (!methodName.empty() && methodName != "new") {
                                diags.addViolation(
                                    "SV-OOP-009",
                                    "EmptyVirtualMethodWithoutPure",
                                    "Virtual method '" + methodName + "' in class '" + className +
                                    "' is defined with an empty body without the 'pure' keyword. Empty stub methods allow derived classes to silently omit required overrides. Declare as 'pure virtual' in interface base classes or provide meaningful default logic.",
                                    core::Severity::Info,
                                    method.declaration->prototype->getFirstToken().location()
                                );
                            }
                        }
                    }
                }
            }
        }

        visitDefault(cls);
    }

private:
    core::DiagnosticEngine& diags;
};

class EmptyVirtualMethodWithoutPure : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-009"; }
    std::string_view getName() const override { return "EmptyVirtualMethodWithoutPure"; }
    std::string_view getDescription() const override {
        return "Flags virtual methods defined with empty bodies instead of being declared 'pure virtual' in abstract base classes.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        EmptyVirtualMethodVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptyVirtualMethodWithoutPure() {
    return std::make_unique<oop::EmptyVirtualMethodWithoutPure>();
}
} // namespace uvmcheck::rules
