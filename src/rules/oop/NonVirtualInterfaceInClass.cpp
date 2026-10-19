#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::oop {

class NonVirtualInterfaceVisitor : public slang::syntax::SyntaxVisitor<NonVirtualInterfaceVisitor> {
public:
    explicit NonVirtualInterfaceVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::string className = std::string(cls.name.valueText());

        for (const auto* member : cls.items) {
            bool hasVirtualQualifier = false;
            const slang::syntax::DataDeclarationSyntax* dataDecl = nullptr;

            if (member->kind == slang::syntax::SyntaxKind::ClassPropertyDeclaration) {
                const auto& prop = member->as<slang::syntax::ClassPropertyDeclarationSyntax>();
                for (const auto& q : prop.qualifiers) {
                    if (q.kind == slang::parsing::TokenKind::VirtualKeyword) {
                        hasVirtualQualifier = true;
                        break;
                    }
                }
                if (prop.declaration && prop.declaration->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                    dataDecl = &prop.declaration->as<slang::syntax::DataDeclarationSyntax>();
                }
            } else if (member->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                dataDecl = &member->as<slang::syntax::DataDeclarationSyntax>();
            }

            if (dataDecl) {
                for (const auto& m : dataDecl->modifiers) {
                    if (m.kind == slang::parsing::TokenKind::VirtualKeyword) {
                        hasVirtualQualifier = true;
                        break;
                    }
                }
                if (dataDecl->type) {
                    if (dataDecl->type->kind == slang::syntax::SyntaxKind::VirtualInterfaceType ||
                        dataDecl->type->getFirstToken().kind == slang::parsing::TokenKind::VirtualKeyword) {
                        hasVirtualQualifier = true;
                    }
                }
            }

            if (dataDecl && dataDecl->type && !hasVirtualQualifier) {
                std::string typeStr = std::string(dataDecl->type->getLastToken().valueText());

                if (!typeStr.starts_with("virtual") &&
                    (typeStr.ends_with("_if") || typeStr.ends_with("_vif"))) {
                    for (const auto* decl : dataDecl->declarators) {
                        std::string varName = std::string(decl->name.valueText());
                        std::vector<core::FixItHint> fixIts;
                        if (dataDecl->type->getFirstToken().location().valid()) {
                            fixIts.push_back({
                                slang::SourceRange(dataDecl->type->getFirstToken().location(),
                                                   dataDecl->type->getFirstToken().location()),
                                "virtual ",
                                "Insert 'virtual' keyword to declare a virtual interface handle"
                            });
                        }

                        diags.addViolation(
                            "SV-OOP-008",
                            "NonVirtualInterfaceInClass",
                            "Interface property '" + varName + "' of type '" + typeStr +
                            "' in class '" + className +
                            "' is declared without the 'virtual' keyword. In SystemVerilog (IEEE 1800 § 25.9), classes cannot instantiate physical interfaces; only virtual interface handles ('virtual " +
                            typeStr + " " + varName + ";') can be stored as class members.",
                            core::Severity::Error,
                            decl->name.location(),
                            fixIts
                        );
                    }
                }
            }
        }

        visitDefault(cls);
    }

private:
    core::DiagnosticEngine& diags;
};

class NonVirtualInterfaceInClass : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-008"; }
    std::string_view getName() const override { return "NonVirtualInterfaceInClass"; }
    std::string_view getDescription() const override {
        return "Disallows declaring physical interface types inside classes without the 'virtual' keyword (IEEE 1800 § 25.9).";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        NonVirtualInterfaceVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonVirtualInterfaceInClass() {
    return std::make_unique<oop::NonVirtualInterfaceInClass>();
}
}
