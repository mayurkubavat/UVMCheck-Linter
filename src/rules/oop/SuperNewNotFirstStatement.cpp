#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include <string>

namespace uvmcheck::rules::oop {

class SuperNewNotFirstStatement : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-014"; }
    std::string_view getName() const override { return "SuperNewNotFirstStatement"; }
    std::string_view getDescription() const override {
        return "Derived class constructors calling super.new(...) must invoke it as the very first procedural statement.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            const auto* base = classType->getBaseClass();
            if (!base || !base->isClass()) continue;

            const auto* ctor = classType->getConstructor();
            if (!ctor || ctor->flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

            const auto* bodyStmt = &ctor->getBody();
            if (bodyStmt->kind == slang::ast::StatementKind::Block) {
                bodyStmt = &bodyStmt->as<slang::ast::BlockStatement>().body;
            }

            if (bodyStmt->kind == slang::ast::StatementKind::List) {
                const auto& stmtList = bodyStmt->as<slang::ast::StatementList>();
                int precedingCount = 0;
                for (const auto* s : stmtList.list) {
                    if (!s || s->kind == slang::ast::StatementKind::Empty) continue;

                    bool isSuperNew = false;
                    if (s->kind == slang::ast::StatementKind::ExpressionStatement) {
                        const auto& exprStmt = s->as<slang::ast::ExpressionStatement>();
                        if (exprStmt.expr.kind == slang::ast::ExpressionKind::NewClass) {
                            const auto& nce = exprStmt.expr.as<slang::ast::NewClassExpression>();
                            if (nce.isSuperClass) {
                                isSuperNew = true;
                            }
                        }
                    }

                    if (isSuperNew) {
                        if (precedingCount > 0) {
                            diags.addViolation(
                                "SV-OOP-014",
                                "SuperNewNotFirstStatement",
                                "Constructor 'new' in derived class '" + std::string(classType->name) +
                                "' invokes 'super.new(...)' after other procedural statements. In SystemVerilog OOP "
                                "(IEEE 1800 § 8.17), 'super.new(...)' must be the "
                                "very first procedural statement in a derived class constructor to ensure base class properties "
                                "and infrastructure are fully initialized before derived logic executes.",
                                core::Severity::Warning,
                                s->sourceRange.start()
                            );
                        }
                        break;
                    }
                    precedingCount++;
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSuperNewNotFirstStatement() {
    return std::make_unique<oop::SuperNewNotFirstStatement>();
}
} // namespace uvmcheck::rules
