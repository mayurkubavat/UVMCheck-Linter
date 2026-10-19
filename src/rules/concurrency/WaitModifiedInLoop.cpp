#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxFacts.h"
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::concurrency {

class IdentifierCollector : public slang::syntax::SyntaxVisitor<IdentifierCollector> {
public:
    std::unordered_set<std::string> ids;

    void handle(const slang::syntax::IdentifierNameSyntax& id) {
        ids.insert(std::string(id.identifier.valueText()));
        visitDefault(id);
    }
};

class LoopAssignmentCollector : public slang::syntax::SyntaxVisitor<LoopAssignmentCollector> {
public:
    std::unordered_set<std::string> modifiedIds;
    bool hasTimingControl = false;

    void handle(const slang::syntax::BinaryExpressionSyntax& bin) {
        if (slang::syntax::SyntaxFacts::isAssignmentOperator(bin.kind)) {
            IdentifierCollector lhsCollector;
            bin.left->visit(lhsCollector);
            modifiedIds.insert(lhsCollector.ids.begin(), lhsCollector.ids.end());
        }
        visitDefault(bin);
    }

    void handle(const slang::syntax::PrefixUnaryExpressionSyntax& un) {
        auto op = un.operatorToken.kind;
        if (op == slang::parsing::TokenKind::DoublePlus ||
            op == slang::parsing::TokenKind::DoubleMinus) {
            IdentifierCollector opCollector;
            un.operand->visit(opCollector);
            modifiedIds.insert(opCollector.ids.begin(), opCollector.ids.end());
        }
        visitDefault(un);
    }

    void handle(const slang::syntax::PostfixUnaryExpressionSyntax& un) {
        auto op = un.operatorToken.kind;
        if (op == slang::parsing::TokenKind::DoublePlus ||
            op == slang::parsing::TokenKind::DoubleMinus) {
            IdentifierCollector opCollector;
            un.operand->visit(opCollector);
            modifiedIds.insert(opCollector.ids.begin(), opCollector.ids.end());
        }
        visitDefault(un);
    }

    void handle(const slang::syntax::TimingControlStatementSyntax& tc) {
        hasTimingControl = true;
        visitDefault(tc);
    }
};

class WaitInLoopVisitor : public slang::syntax::SyntaxVisitor<WaitInLoopVisitor> {
public:
    explicit WaitInLoopVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void checkLoop(const slang::syntax::SyntaxNode& loopBody,
                   slang::SourceLocation loc) {
        // Find any wait statements in this loop
        struct WaitFinder : public slang::syntax::SyntaxVisitor<WaitFinder> {
            std::vector<const slang::syntax::WaitStatementSyntax*> waits;
            void handle(const slang::syntax::WaitStatementSyntax& ws) {
                waits.push_back(&ws);
                visitDefault(ws);
            }
        } finder;

        loopBody.visit(finder);
        if (finder.waits.empty()) return;

        // Collect all assignments in the loop body
        LoopAssignmentCollector assignCollector;
        loopBody.visit(assignCollector);

        for (const auto* ws : finder.waits) {
            IdentifierCollector condIds;
            ws->expr->visit(condIds);

            if (condIds.ids.empty()) continue;

            bool isAnyModified = false;
            for (const auto& id : condIds.ids) {
                if (assignCollector.modifiedIds.count(id) > 0) {
                    isAnyModified = true;
                    break;
                }
            }

            if (!isAnyModified) {
                std::string idList;
                size_t count = 0;
                for (const auto& id : condIds.ids) {
                    if (count > 0) idList += ", ";
                    idList += "'" + id + "'";
                    count++;
                }

                diags.addViolation(
                    "SV-CON-011",
                    "WaitModifiedInLoop",
                    "Condition variables (" + idList + ") in 'wait(...)' statement are not modified or assigned anywhere inside the loop body. "
                    "In SystemVerilog concurrency guidelines (IEEE 1800 § 9.4.4), waiting inside a loop without mutating condition variables risks "
                    "zero-delay infinite spinning or unblocked iteration once the condition becomes true.",
                    core::Severity::Warning,
                    ws->wait.location()
                );
            }
        }
    }

    void handle(const slang::syntax::ForLoopStatementSyntax& loop) {
        checkLoop(loop, loop.forKeyword.location());
        visitDefault(loop);
    }

    void handle(const slang::syntax::LoopStatementSyntax& loop) {
        checkLoop(loop, loop.repeatOrWhile.location());
        visitDefault(loop);
    }

    void handle(const slang::syntax::DoWhileStatementSyntax& loop) {
        checkLoop(loop, loop.doKeyword.location());
        visitDefault(loop);
    }

    void handle(const slang::syntax::ForeverStatementSyntax& loop) {
        checkLoop(loop, loop.foreverKeyword.location());
        visitDefault(loop);
    }

private:
    core::DiagnosticEngine& diags;
};

class WaitModifiedInLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-011"; }
    std::string_view getName() const override { return "WaitModifiedInLoop"; }
    std::string_view getDescription() const override {
        return "Flags wait(...) statements inside loops where condition variables are never modified or assigned inside the loop body.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        WaitInLoopVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createWaitModifiedInLoop() {
    return std::make_unique<concurrency::WaitModifiedInLoop>();
}
} // namespace uvmcheck::rules
