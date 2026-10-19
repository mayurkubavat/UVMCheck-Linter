#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/syntax/AllSyntax.h"
#include <string>
#include <vector>
#include <unordered_set>

namespace uvmcheck::rules::uvm {

struct FifoDecl {
    std::string parentClassName;
    std::string fifoName;
    slang::SourceLocation location;
};

class AnalysisFifoDrainStarvation : public Rule {
public:
    std::string_view getId() const override { return "UVM-TLM-006"; }
    std::string_view getName() const override { return "AnalysisFifoDrainStarvation"; }
    std::string_view getDescription() const override {
        return "Detects instantiated uvm_tlm_analysis_fifo components that receive transactions via "
               "connect_phase but have zero drain operations (.get(), .try_get()) across all task-based "
               "phases, causing unbounded simulation heap memory growth and fatal bad_alloc crashes.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        std::vector<FifoDecl> declaredFifos;

        // Pass 1: Discover all declared uvm_tlm_analysis_fifo instances
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmComponent(*cls)) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    std::string typeStr(prop.getType().toString());

                    if (typeStr.find("tlm_analysis_fifo") != std::string::npos ||
                        typeStr.find("tlm_fifo") != std::string::npos) {
                        declaredFifos.push_back({
                            std::string(cls->name),
                            std::string(prop.name),
                            prop.location
                        });
                    }
                }
            }
        }

        if (declaredFifos.empty()) return;

        // Pass 2: Verify which FIFOs are connected to receive transactions in connect_phase
        std::unordered_set<std::string> connectedFifos;

        class ConnectVisitor : public slang::ast::ASTVisitor<ConnectVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            std::unordered_set<std::string>& connected;
            const std::vector<FifoDecl>& fifos;

            ConnectVisitor(std::unordered_set<std::string>& c, const std::vector<FifoDecl>& f)
                : connected(c), fifos(f) {}

            void handle(const slang::ast::CallExpression& call) {
                if (call.getSubroutineName() == "connect" && call.syntax) {
                    std::string text(call.syntax->toString());
                    for (const auto& f : fifos) {
                        if (text.find(f.fifoName) != std::string::npos) {
                            connected.insert(f.fifoName);
                        }
                    }
                }
                visitDefault(call);
            }
        };

        ConnectVisitor connVisitor(connectedFifos, declaredFifos);
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "connect_phase") {
                        sub.getBody().visit(connVisitor);
                    }
                }
            }
        }

        // Pass 3: Check if connected FIFOs are ever drained in any task phase
        std::unordered_set<std::string> drainedFifos;

        class DrainVisitor : public slang::ast::ASTVisitor<DrainVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            std::unordered_set<std::string>& drained;
            const std::vector<FifoDecl>& fifos;

            DrainVisitor(std::unordered_set<std::string>& d, const std::vector<FifoDecl>& f)
                : drained(d), fifos(f) {}

            void handle(const slang::ast::CallExpression& call) {
                std::string subName(call.getSubroutineName());
                if (subName == "get" || subName == "try_get" || subName == "peek" || subName == "try_peek") {
                    if (call.syntax) {
                        std::string text(call.syntax->toString());
                        for (const auto& f : fifos) {
                            if (text.find(f.fifoName) != std::string::npos) {
                                drained.insert(f.fifoName);
                            }
                        }
                    }
                }
                visitDefault(call);
            }
        };

        DrainVisitor drainVisitor(drainedFifos, declaredFifos);
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.subroutineKind == slang::ast::SubroutineKind::Task) {
                        sub.getBody().visit(drainVisitor);
                    }
                }
            }
        }

        // Pass 4: Flag connected FIFOs that have no drain operations
        for (const auto& f : declaredFifos) {
            if (connectedFifos.count(f.fifoName) > 0 && drainedFifos.count(f.fifoName) == 0) {
                diags.addViolation(
                    "UVM-TLM-006",
                    "AnalysisFifoDrainStarvation",
                    "Analysis FIFO '" + f.fifoName + "' in component '" + f.parentClassName +
                    "' is connected to receive transactions in connect_phase, but is never drained (.get() / .try_get()) "
                    "in any task phase across the verification environment. "
                    "Unconsumed transactions will queue indefinitely, causing unbounded memory bloat.",
                    core::Severity::Warning,
                    f.location
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAnalysisFifoDrainStarvation() {
    return std::make_unique<uvm::AnalysisFifoDrainStarvation>();
}
}
