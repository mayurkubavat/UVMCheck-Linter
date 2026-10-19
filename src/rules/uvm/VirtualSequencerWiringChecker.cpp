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

class VirtualSequencerWiringChecker : public Rule {
public:
    std::string_view getId() const override { return "UVM-VSEQ-001"; }
    std::string_view getName() const override { return "VirtualSequencerWiringChecker"; }
    std::string_view getDescription() const override {
        return "Statically verifies that sub-sequencer handles declared within a virtual sequencer "
               "are fully wired and assigned in connect_phase across the verification environment.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        struct SubSeqrField {
            std::string parentClassName;
            std::string fieldName;
            slang::SourceLocation location;
        };

        std::vector<SubSeqrField> subSeqrFields;

        // Step 1: Identify virtual sequencers and their sub-sequencer member handles
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            std::string clsName(cls->name);
            bool isVirtualSeqr = (clsName.find("vseqr") != std::string::npos ||
                                  clsName.find("virtual_sequencer") != std::string::npos ||
                                  uvmModel.isDerivedFrom(*cls, "uvm_sequencer"));

            if (!isVirtualSeqr) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    std::string typeStr(prop.getType().toString());
                    std::string propName(prop.name);

                    // If this property holds another sequencer
                    bool isSubSequencer = (typeStr.find("sequencer") != std::string::npos ||
                                           propName.find("_seqr") != std::string::npos ||
                                           propName.find("sequencer") != std::string::npos);

                    if (isSubSequencer && !uvmModel.isDerivedFrom(prop.getType(), "uvm_phase")) {
                        subSeqrFields.push_back({clsName, propName, prop.location});
                    }
                }
            }
        }

        if (subSeqrFields.empty()) return;

        // Step 2: Traverse all connect_phase functions and collect LHS assignments
        std::vector<std::string> connectAssignments;

        class ConnectPhaseAssignmentVisitor : public slang::ast::ASTVisitor<ConnectPhaseAssignmentVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            std::vector<std::string>& assignments;
            bool inConnectPhase = false;

            explicit ConnectPhaseAssignmentVisitor(std::vector<std::string>& outAssigns) : assignments(outAssigns) {}

            void handle(const slang::ast::SubroutineSymbol& sub) {
                if (sub.name == "connect_phase") {
                    bool prev = inConnectPhase;
                    inConnectPhase = true;
                    visitDefault(sub);
                    inConnectPhase = prev;
                } else {
                    visitDefault(sub);
                }
            }

            void handle(const slang::ast::AssignmentExpression& assign) {
                if (inConnectPhase && assign.syntax) {
                    assignments.push_back(std::string(assign.syntax->toString()));
                }
                visitDefault(assign);
            }
        };

        ConnectPhaseAssignmentVisitor visitor(connectAssignments);
        compilation.getRoot().visit(visitor);

        // Step 3: Flag sub-sequencer fields that are never assigned in connect_phase
        for (const auto& field : subSeqrFields) {
            bool assigned = false;
            for (const auto& stmt : connectAssignments) {
                if (stmt.find(field.fieldName) != std::string::npos) {
                    assigned = true;
                    break;
                }
            }

            if (!assigned) {
                diags.addViolation(
                    "UVM-VSEQ-001",
                    "VirtualSequencerWiringChecker",
                    "Sub-sequencer handle '" + field.fieldName + "' in virtual sequencer '" +
                    field.parentClassName + "' is declared but never wired in any connect_phase. "
                    "Virtual sequences attempting to access p_sequencer." + field.fieldName +
                    " will crash with a null-pointer dereference at runtime.",
                    core::Severity::Warning,
                    field.location
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createVirtualSequencerWiringChecker() {
    return std::make_unique<uvm::VirtualSequencerWiringChecker>();
}
}
