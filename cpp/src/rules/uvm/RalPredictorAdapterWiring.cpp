#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

// Assignments made in any connect_phase, in any class (extern bodies and
// parameterized classes included): "lhs = rhs" as written.
inline std::vector<std::string> connectPhaseAssignments(const model::Model& design) {
    std::vector<std::string> out;
    for (const model::Class& cls : design.classes())
        for (const model::Method& m : cls.methods)
            if (m.name == "connect_phase")
                for (const model::Assignment& a : m.assignments) out.push_back(a.lhs + " = " + a.rhs);
    return out;
}

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

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        struct PredictorDecl {
            const model::Class* owner;
            const model::Property* prop;
        };
        std::vector<PredictorDecl> predictors;
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            if (!model::isComponent(cls.uvm)) continue;
            for (const model::Property& prop : cls.properties)
                if (prop.type.find("reg_predictor") != std::string::npos) predictors.push_back({&cls, &prop});
        }
        if (predictors.empty()) return;

        const auto assignments = connectPhaseAssignments(design);
        for (const auto& pred : predictors) {
            const std::string mapTarget = pred.prop->name + ".map";
            bool mapBound = false;
            for (const auto& stmt : assignments) {
                if (stmt.find(mapTarget) != std::string::npos) {
                    mapBound = true;
                    break;
                }
            }
            if (mapBound) continue;
            diags.addViolation(
                "RAL-WIRE-001",
                "RalPredictorAdapterWiring",
                "Register predictor '" + pred.prop->name + "' in component '" + pred.owner->name +
                "' is missing mandatory '.map' assignment in connect_phase (e.g. '" +
                pred.prop->name + ".map = reg_model.default_map;'). "
                "Without map assignment, register prediction will fail silently and mirror values will not update.",
                core::Severity::Warning,
                model::toSlang(pred.prop->loc)
            );
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRalPredictorAdapterWiring() {
    return std::make_unique<uvm::RalPredictorAdapterWiring>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRalPredictorAdapterWiring)
