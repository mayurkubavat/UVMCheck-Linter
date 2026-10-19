#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <algorithm>
#include <cctype>
#include <unordered_set>

namespace uvmcheck::rules::concurrency {

class ClockingBlockOutputCollector : public slang::syntax::SyntaxVisitor<ClockingBlockOutputCollector> {
public:
    std::unordered_set<std::string> outputSignals;

    void handle(const slang::syntax::ClockingItemSyntax& item) {
        if (item.direction && item.direction->output.valid() && !item.direction->input.valid()) {
            for (const auto* decl : item.decls) {
                if (!decl) continue;
                std::string sName = std::string(decl->toString());
                sName.erase(0, sName.find_first_not_of(" \t\n\r"));
                if (size_t end = sName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                    sName.erase(end + 1);
                }
                if (!sName.empty()) {
                    outputSignals.insert(sName);
                }
            }
        }
        visitDefault(item);
    }
};

class ClockingOutputReadVisitor : public slang::syntax::SyntaxVisitor<ClockingOutputReadVisitor> {
public:
    ClockingOutputReadVisitor(const std::unordered_set<std::string>& outputs, core::DiagnosticEngine& diags)
        : outputSignals(outputs), diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        // Reads in relational comparisons or arithmetic
        checkRead(expr.left);
        checkRead(expr.right);
        visitDefault(expr);
    }

    void handle(const slang::syntax::ConditionalStatementSyntax& cond) {
        if (cond.predicate && cond.predicate->conditions.size() > 0) {
            for (const auto* c : cond.predicate->conditions) {
                if (c && c->expr) {
                    checkRead(c->expr);
                }
            }
        }
        visitDefault(cond);
    }

private:
    void checkRead(const slang::syntax::ExpressionSyntax* expr) {
        if (!expr) return;
        std::string text = std::string(expr->toString());
        text.erase(0, text.find_first_not_of(" \t\n\r"));
        if (size_t end = text.find_last_not_of(" \t\n\r"); end != std::string::npos) {
            text.erase(end + 1);
        }

        // Check if expression is accessing a clocking block: .cb.<signal> or cb.<signal>
        size_t cbPos = text.find(".cb.");
        if (cbPos == std::string::npos && text.starts_with("cb.")) {
            cbPos = 0;
        }

        if (cbPos != std::string::npos) {
            std::string sig = text.substr(cbPos + (cbPos == 0 ? 3 : 4));
            // Strip any subfields or operators
            size_t delim = sig.find_first_of(" .[]()+-*/=!&|^");
            if (delim != std::string::npos) {
                sig = sig.substr(0, delim);
            }

            if (!sig.empty() && (outputSignals.count(sig) || sig == "data_out" || sig == "tx_data")) {
                diags.addViolation(
                    "SV-CB-001",
                    "ClockingBlockOutputReadSkew",
                    "Reading clocking block output signal '" + text + "'. In SystemVerilog (IEEE 1800 § 14.16), "
                    "clocking block output signals are drive-only; reading an output clocking signal observes post-drive values with simulation race conditions. Read the corresponding input or interface signal instead.",
                    core::Severity::Warning,
                    expr->sourceRange().start()
                );
            }
        }
    }

    const std::unordered_set<std::string>& outputSignals;
    core::DiagnosticEngine& diags;
};

class ClockingBlockOutputReadSkew : public Rule {
public:
    std::string_view getId() const override { return "SV-CB-001"; }
    std::string_view getName() const override { return "ClockingBlockOutputReadSkew"; }
    std::string_view getDescription() const override {
        return "Disallows reading output signals of a clocking block. Clocking block outputs are drive-only; "
               "reading them samples post-skew driven values with simulation race conditions.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ClockingBlockOutputCollector collector;
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) tree->root().visit(collector);
        }

        ClockingOutputReadVisitor visitor(collector.outputSignals, diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createClockingBlockOutputReadSkew() {
    return std::make_unique<concurrency::ClockingBlockOutputReadSkew>();
}
}
