#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>

namespace uvmcheck::rules::oop {

class ReturnFinderVisitor : public slang::syntax::SyntaxVisitor<ReturnFinderVisitor> {
public:
    std::vector<const slang::syntax::ReturnStatementSyntax*> returns;

    void handle(const slang::syntax::ReturnStatementSyntax& stmt) {
        returns.push_back(&stmt);
        visitDefault(stmt);
    }
};

class MutableGetterVisitor : public slang::syntax::SyntaxVisitor<MutableGetterVisitor> {
public:
    explicit MutableGetterVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::unordered_set<std::string> objectMembers;

        // Collect class members that are user/class object types
        for (const auto* memberNode : cls.items) {
            const slang::syntax::DataDeclarationSyntax* dataDecl = nullptr;
            if (memberNode->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                dataDecl = &memberNode->as<slang::syntax::DataDeclarationSyntax>();
            } else if (memberNode->kind == slang::syntax::SyntaxKind::ClassPropertyDeclaration) {
                const auto& classProp = memberNode->as<slang::syntax::ClassPropertyDeclarationSyntax>();
                if (classProp.declaration->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                    dataDecl = &classProp.declaration->as<slang::syntax::DataDeclarationSyntax>();
                }
            }

            if (dataDecl) {
                std::string typeStr = std::string(dataDecl->type->toString());
                typeStr.erase(0, typeStr.find_first_not_of(" \t\n\r"));
                if (size_t end = typeStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                    typeStr.erase(end + 1);
                }

                if (!isPrimitiveType(typeStr)) {
                    for (const auto* decl : dataDecl->declarators) {
                        objectMembers.insert(std::string(decl->name.valueText()));
                    }
                }
            }
        }

        // Check getter functions for direct returns of object members
        for (const auto* memberNode : cls.items) {
            const slang::syntax::FunctionDeclarationSyntax* funcDecl = nullptr;
            if (memberNode->kind == slang::syntax::SyntaxKind::FunctionDeclaration) {
                funcDecl = &memberNode->as<slang::syntax::FunctionDeclarationSyntax>();
            } else if (memberNode->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
                const auto& classMethod = memberNode->as<slang::syntax::ClassMethodDeclarationSyntax>();
                funcDecl = classMethod.declaration.get();
            }

            if (funcDecl) {
                std::string funcName = std::string(funcDecl->prototype->name->getLastToken().valueText());

                if (funcName.starts_with("get_") || (funcName.starts_with("get") && funcName.size() > 3 && std::isupper(funcName[3]))) {
                    ReturnFinderVisitor retFinder;
                    for (const auto* item : funcDecl->items) {
                        item->visit(retFinder);
                    }

                    for (const auto* retStmt : retFinder.returns) {
                        if (retStmt->returnValue && retStmt->returnValue->kind == slang::syntax::SyntaxKind::IdentifierName) {
                            std::string retVar = std::string(retStmt->returnValue->as<slang::syntax::IdentifierNameSyntax>().identifier.valueText());
                            if (objectMembers.count(retVar) > 0) {
                                diags.addViolation(
                                    "SV-ENC-001",
                                    "MutableObjectGetterLeak",
                                    "Getter method '" + funcName + "()' returns direct reference to mutable member object '" + retVar + "'. "
                                    "Returning handles to internal mutable class objects breaks OOP encapsulation and allows callers to mutate internal state without ownership control. "
                                    "Return a cloned copy of the object (e.g. 'return " + retVar + ".clone();') or ensure the object is immutable.",
                                    core::Severity::Warning,
                                    retStmt->returnKeyword.location()
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

    static bool isPrimitiveType(std::string_view typeStr) {
        return typeStr == "int" || typeStr == "integer" || typeStr == "bit" ||
               typeStr == "logic" || typeStr == "reg" || typeStr == "byte" ||
               typeStr == "shortint" || typeStr == "longint" || typeStr == "time" ||
               typeStr == "real" || typeStr == "shortreal" || typeStr == "realtime" ||
               typeStr == "string" || typeStr == "void" || typeStr == "chandle" ||
               typeStr == "event";
    }
};

class MutableObjectGetterLeak : public Rule {
public:
    std::string_view getId() const override { return "SV-ENC-001"; }
    std::string_view getName() const override { return "MutableObjectGetterLeak"; }
    std::string_view getDescription() const override {
        return "Disallows getter functions (get_*()) from returning direct references to mutable internal class object members. "
               "Returning mutable handles leaks encapsulation; return a clone or copy instead.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        MutableGetterVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMutableObjectGetterLeak() {
    return std::make_unique<oop::MutableObjectGetterLeak>();
}
}
