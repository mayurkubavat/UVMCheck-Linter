#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

namespace {

// Default testbench layers, lowest first. Classes outside these roles (config objects,
// register models, plain classes) are shared by all layers and not checked.
int layerOf(swe::Role role) {
    switch (role) {
        case swe::Role::Item: return 0;
        case swe::Role::Sequence: return 1;
        case swe::Role::Driver: case swe::Role::Monitor: case swe::Role::Sequencer:
        case swe::Role::Agent: case swe::Role::Subscriber:
            return 2;
        case swe::Role::Scoreboard: case swe::Role::Env:
            return 3;
        case swe::Role::Test:
            return 4;
        default:
            return -1;
    }
}

const char* layerName(int layer) {
    switch (layer) {
        case 0: return "sequence item";
        case 1: return "sequence";
        case 2: return "agent";
        case 3: return "env";
        case 4: return "test";
        default: return "?";
    }
}

} // namespace

class LayerDependency : public Rule {
public:
    std::string_view getId() const override { return "UVM-ARC-001"; }
    std::string_view getName() const override { return "LayerDependency"; }
    std::string_view getDescription() const override {
        return "Flags classes that depend on a higher testbench layer (item < sequence < agent < env < "
               "test), such as a sequence that references a scoreboard or an agent that references the "
               "env (ArchUnit layer rules).";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto& cm : metrics::classMetrics(compilation, model)) {
            swe::Role role = swe::classRole(*cm.cls);
            int mine = layerOf(role);
            if (mine < 0) continue;
            for (const auto* dep : cm.coupledClasses) {
                swe::Role depRole = swe::classRole(*dep);
                int theirs = layerOf(depRole);
                if (theirs <= mine) continue;
                // A sequence naming its sequencer type (p_sequencer, virtual sequencers) is UVM-SEQ-003's job.
                if (role == swe::Role::Sequence && depRole == swe::Role::Sequencer) continue;
                std::string depName(dep->genericClass ? dep->genericClass->name : dep->name);
                diags.addViolation(
                    "UVM-ARC-001", "LayerDependency",
                    std::string(swe::roleName(role)) + " '" + std::string(cm.cls->name) + "' (" +
                        layerName(mine) + " layer) depends on " + swe::roleName(depRole) + " '" + depName +
                        "' (" + layerName(theirs) + " layer). Lower layers must not know about the "
                        "layers that use them, or the " + layerName(mine) +
                        " code cannot be reused in another env. Pass data down through config objects, "
                        "TLM ports or callbacks instead.",
                    core::Severity::Warning, cm.cls->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLayerDependency() {
    return std::make_unique<design::LayerDependency>();
}
}
