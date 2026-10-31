// UVM-OBS-002: analysis write() implementations must finish in zero time and must not
// spawn a thread per transaction.
//
// Only real analysis implementations are checked: write() of a uvm_subscriber, and
// write<SFX>() of a class that owns a uvm_analysis_imp<SFX> (the classes
// `uvm_analysis_imp_decl(SFX) generates). A driver's own write_reg() task is not an
// observer and is left alone. Delays, waits and task calls inside a function are already
// compile errors, so this rule reports the two mistakes that compile or that the compiler
// reports far away, inside the library:
//   - write is declared as a task;
//   - write forks threads (fork ... join_none), one per broadcast transaction.

#include <set>
#include <string>

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::uvm {

using namespace slang::ast;

namespace {

constexpr std::string_view kImpPrefix = "uvm_analysis_imp";

/// Names of the analysis write methods `cls` implements.
std::set<std::string> analysisWriteMethods(const ClassType& cls) {
    std::set<std::string> names;
    if (swe::classRole(cls) == swe::Role::Subscriber) names.insert("write");

    for (const auto& member : cls.members()) {
        if (member.kind != SymbolKind::ClassProperty) continue;
        const auto* propCls = swe::asClass(member.as<ClassPropertySymbol>().getType());
        if (!propCls) continue;
        std::string_view n = propCls->name;
        if (!n.starts_with(kImpPrefix)) continue;
        // uvm_analysis_imp -> write; uvm_analysis_imp_foo (from `uvm_analysis_imp_decl(_foo))
        // -> write_foo.
        names.insert("write" + std::string(n.substr(kImpPrefix.size())));
    }
    return names;
}

struct ForkFinder : ASTVisitor<ForkFinder, VisitFlags::AllGood> {
    const BlockStatement* found = nullptr;
    void handle(const BlockStatement& block) {
        if (!found && block.blockKind == StatementBlockKind::JoinNone) {
            found = &block;
            return;
        }
        visitDefault(block);
    }
};

} // namespace

class BlockingInObserver : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBS-002"; }
    std::string_view getName() const override { return "BlockingInObserver"; }
    std::string_view getDescription() const override {
        return "Analysis write() implementations (uvm_subscriber::write and the write<SFX> methods "
               "behind uvm_analysis_imp<SFX>) must be functions that finish in zero time. Flags a "
               "write declared as a task, and a write that forks a thread per transaction.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    // Queries the class model only; it does not walk the design.
    void run(slang::ast::Compilation&, const uvmcheck::uvm::UvmModel& model,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            auto writes = analysisWriteMethods(*cls);
            if (writes.empty()) continue;

            for (const auto* sub : swe::classMethods(*cls)) {
                if (!writes.contains(std::string(sub->name))) continue;
                const std::string where = "'" + std::string(sub->name) + "' of class '" +
                                          std::string(cls->name) + "'";

                if (sub->subroutineKind == SubroutineKind::Task) {
                    diags.addViolation(
                        "UVM-OBS-002", "BlockingInObserver",
                        "Analysis method " + where + " is declared as a task. Analysis ports call "
                        "write() as a function in zero time; make it a void function and move "
                        "time-consuming work to run_phase() behind a uvm_tlm_analysis_fifo.",
                        core::Severity::Error, sub->location);
                    continue;
                }

                ForkFinder finder;
                sub->getBody().visit(finder);
                if (finder.found) {
                    diags.addViolation(
                        "UVM-OBS-002", "BlockingInObserver",
                        "Analysis method " + where + " forks a thread for every transaction "
                        "(fork ... join_none). Threads pile up under load and transactions lose "
                        "their order. Push the transaction into a uvm_tlm_analysis_fifo and "
                        "process it from one run_phase() thread.",
                        core::Severity::Warning,
                        finder.found->syntax ? finder.found->syntax->getFirstToken().location()
                                             : finder.found->sourceRange.start());
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createBlockingInObserver() {
    return std::make_unique<uvm::BlockingInObserver>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createBlockingInObserver)
