#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class RalPredictorAdapterWiring : public Rule {
public:
    std::string_view getId() const override { return "RAL-WIRE-001"; }
    std::string_view getName() const override { return "RalPredictorAdapterWiring"; }
    std::string_view getDescription() const override {
        return "Verifies that all instantiated uvm_reg_predictor components have their .map property "
               "assigned in connect_phase. Without a map binding, explicit register prediction fails "
               "silently and register mirror values will never be updated during simulation.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        struct PredictorDecl {
            std::string parentClassName;
            std::string predictorName;
            slang::SourceLocation location;
        };

        std::vector<PredictorDecl> predictors;

        // Step 1: Discover all declared uvm_reg_predictor instances
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmComponent(*cls)) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    std::string typeStr(prop.getType().toString());

                    if (typeStr.find("uvm_reg_predictor") != std::string::npos ||
                        typeStr.find("reg_predictor") != std::string::npos) {
                        predictors.push_back({
                            std::string(cls->name),
                            std::string(prop.name),
                            prop.location
                        });
                    }
                }
            }
        }

        if (predictors.empty()) return;

        // Step 2: Collect connect_phase statements
        std::vector<std::string> connectStatements;

        class ConnectVisitor : public slang::ast::ASTVisitor<ConnectVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            std::vector<std::string>& stmts;
            bool insideConnectPhase = false;

            explicit ConnectVisitor(std::vector<std::string>& outStmts) : stmts(outStmts) {}

            void handle(const slang::ast::SubroutineSymbol& sub) {
                if (sub.name == "connect_phase") {
                    bool prev = insideConnectPhase;
                    insideConnectPhase = true;
                    visitDefault(sub);
                    insideConnectPhase = prev;
                } else {
                    visitDefault(sub);
                }
            }

            void handle(const slang::ast::AssignmentExpression& assign) {
                if (insideConnectPhase && assign.syntax) {
                    stmts.push_back(std::string(assign.syntax->toString()));
                }
                visitDefault(assign);
            }
        };

        ConnectVisitor visitor(connectStatements);
        compilation.getRoot().visit(visitor);

        // Step 3: Verify each predictor has its .map property bound
        for (const auto& pred : predictors) {
            std::string mapTarget = pred.predictorName + ".map";
            bool mapBound = false;

            for (const auto& stmt : connectStatements) {
                if (stmt.find(mapTarget) != std::string::npos) {
                    mapBound = true;
                    break;
                }
            }

            if (!mapBound) {
                diags.addViolation(
                    "RAL-WIRE-001",
                    "RalPredictorAdapterWiring",
                    "Register predictor '" + pred.predictorName + "' in component '" + pred.parentClassName +
                    "' is missing mandatory '.map' assignment in connect_phase (e.g. '" +
                    pred.predictorName + ".map = reg_model.default_map;'). "
                    "Without map assignment, register prediction will fail silently and mirror values will not update.",
                    core::Severity::Warning,
                    pred.location
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRalPredictorAdapterWiring() {
    return std::make_unique<uvm::RalPredictorAdapterWiring>();
}
}
