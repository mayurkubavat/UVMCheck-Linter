#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>

namespace uvmcheck::rules::security {

class UncheckedFopenVisitor : public slang::syntax::SyntaxVisitor<UncheckedFopenVisitor> {
public:
    explicit UncheckedFopenVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& node) {
        if (node.left) {
            std::string callee = std::string(node.left->toString());
            callee.erase(0, callee.find_first_not_of(" \t\n\r"));
            if (size_t end = callee.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                callee.erase(end + 1);
            }

            if (callee == "$fopen") {
                const auto* p = node.parent.get();
                bool isDiscarded = false;

                if (p && p->kind == slang::syntax::SyntaxKind::ExpressionStatement) {
                    isDiscarded = true;
                } else if (p && p->kind == slang::syntax::SyntaxKind::CastExpression) {
                    const auto* grandParent = p->parent.get();
                    if (grandParent && grandParent->kind == slang::syntax::SyntaxKind::ExpressionStatement) {
                        isDiscarded = true;
                    }
                }

                if (isDiscarded) {
                    diags.addViolation(
                        "SV-SYS-001",
                        "UncheckedFopen",
                        "Return descriptor of '$fopen' is discarded. In SystemVerilog (IEEE 1800 § 21.3.1), '$fopen' returns a 32-bit descriptor; "
                        "discarding the return value leaks file handles and prevents error validation.",
                        core::Severity::Warning,
                        node.sourceRange().start(),
                        {}
                    );
                } else if (p && p->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                    // Check if assigned: fd = $fopen(...)
                    const auto& bin = p->as<slang::syntax::BinaryExpressionSyntax>();
                    if (bin.operatorToken.kind == slang::parsing::TokenKind::Equals) {
                        std::string varName = std::string(bin.left->getLastToken().valueText());

                        // Inspect enclosing block/subroutine for checks on varName
                        const slang::syntax::SyntaxNode* enclosing = bin.parent.get();
                        while (enclosing &&
                               enclosing->kind != slang::syntax::SyntaxKind::FunctionDeclaration &&
                               enclosing->kind != slang::syntax::SyntaxKind::TaskDeclaration &&
                               enclosing->kind != slang::syntax::SyntaxKind::SequentialBlockStatement &&
                               enclosing->kind != slang::syntax::SyntaxKind::ModuleDeclaration) {
                            enclosing = enclosing->parent.get();
                        }

                        if (enclosing) {
                            class ConditionChecker : public slang::syntax::SyntaxVisitor<ConditionChecker> {
                            public:
                                explicit ConditionChecker(std::string_view target) : target(target) {}
                                bool foundCheck = false;

                                void handle(const slang::syntax::ConditionalStatementSyntax& cond) {
                                    if (cond.predicate) {
                                        std::string condText = std::string(cond.predicate->toString());
                                        if (condText.find(target) != std::string::npos) {
                                            foundCheck = true;
                                        }
                                    }
                                    visitDefault(cond);
                                }
                            private:
                                std::string_view target;
                            };

                            ConditionChecker condChecker(varName);
                            enclosing->visit(condChecker);

                            if (!condChecker.foundCheck) {
                                diags.addViolation(
                                    "SV-SYS-001",
                                    "UncheckedFopen",
                                    "File descriptor '" + varName +
                                    "' returned by '$fopen' is not verified against 0/NULL (e.g. 'if (" + varName +
                                    " == 0)'). If the file cannot be opened, subsequent writes will crash the simulator or result in lost log data.",
                                    core::Severity::Warning,
                                    node.sourceRange().start(),
                                    {}
                                );
                            }
                        }
                    }
                }
            }
        }
        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class UncheckedFopen : public Rule {
public:
    std::string_view getId() const override { return "SV-SYS-001"; }
    std::string_view getName() const override { return "UncheckedFopen"; }
    std::string_view getDescription() const override {
        return "Flags unchecked $fopen return descriptors and discarded $fopen calls. "
               "File operations must check whether the descriptor is valid before performing file I/O.";
    }
    std::string_view getCategory() const override { return "Security"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UncheckedFopenVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::security

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUncheckedFopen() {
    return std::make_unique<security::UncheckedFopen>();
}
}
