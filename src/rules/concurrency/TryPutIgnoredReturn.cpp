#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::concurrency {

class TryPutIgnoredVisitor : public slang::ast::ASTVisitor<TryPutIgnoredVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit TryPutIgnoredVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ExpressionStatement& stmt) {
        const auto* expr = &stmt.expr;
        while (expr->kind == slang::ast::ExpressionKind::Conversion) {
            expr = &expr->as<slang::ast::ConversionExpression>().operand();
        }

        if (expr->kind == slang::ast::ExpressionKind::Call) {
            const auto& call = expr->as<slang::ast::CallExpression>();
            std::string subName = std::string(call.getSubroutineName());
            if (subName == "try_put" || subName == "try_get" || subName == "try_peek") {
                if (subName == "try_get" && call.thisClass()) {
                    std::string typeStr = call.thisClass()->type->toString();
                    if (typeStr == "semaphore" || typeStr.find("semaphore") != std::string::npos) {
                        visitDefault(stmt);
                        return;
                    }
                }
                diags.addViolation(
                    "SV-IPC-001",
                    "TryPutIgnoredReturn",
                    "The return value of non-blocking IPC method '" + subName + "()' is ignored or cast to void. "
                    "Non-blocking IPC calls return 1 on success and 0 when the FIFO or mailbox is full or empty. "
                    "Discarding the return value causes transactions to be silently lost without warning. "
                    "Check the return value with 'if (!fifo." + subName + "(...))' or use blocking 'put()'.",
                    core::Severity::Warning,
                    call.sourceRange.start()
                );
            }
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class TryPutIgnoredReturn : public Rule {
public:
    std::string_view getId() const override { return "SV-IPC-001"; }
    std::string_view getName() const override { return "TryPutIgnoredReturn"; }
    std::string_view getDescription() const override {
        return "Non-blocking IPC routines (try_put, try_get, try_peek) return a boolean status. "
               "Ignoring the return value silently drops transactions when queues are full or empty.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        TryPutIgnoredVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTryPutIgnoredReturn() {
    return std::make_unique<concurrency::TryPutIgnoredReturn>();
}
}
