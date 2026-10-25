// Methodology guide chapter MG-3, cohesion and component roles: UVM-ROL-001 to UVM-ROL-005.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include <cctype>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;

namespace {

// The object a method call works on: obj for obj.randomize() / obj.start(...).
const Expression* receiverOf(const CallExpression& call) {
    if (call.thisClass()) return call.thisClass();
    // Built-in methods such as randomize() carry the object as their first argument.
    if (call.isSystemCall() && !call.arguments().empty()) return call.arguments()[0];
    return nullptr;
}

swe::Role receiverRole(const CallExpression& call) {
    const Expression* r = receiverOf(call);
    return r && r->type ? meth::roleOfType(*r->type) : swe::Role::None;
}

std::string lower(std::string_view s) {
    std::string out(s);
    for (char& c : out) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    return out;
}

} // namespace

// UVM-ROL-001 -----------------------------------------------------------------------------

class DriverGeneratesStimulus : public Rule {
public:
    std::string_view getId() const override { return "UVM-ROL-001"; }
    std::string_view getName() const override { return "DriverGeneratesStimulus"; }
    std::string_view getDescription() const override {
        return "Flags drivers that randomize sequence items or start sequences. Stimulus belongs in "
               "sequences, where tests can layer constraints and the sequencer can arbitrate; a driver "
               "only converts items to pin activity (single responsibility, MG-3.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Driver) continue;
            for (const auto* m : swe::classMethods(*cls)) {
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    std::string_view n = call.getSubroutineName();
                    swe::Role r = receiverRole(call);
                    std::string what;
                    if (n == "randomize" && (r == swe::Role::Item || r == swe::Role::Sequence)) {
                        what = "randomizes " + std::string(swe::roleName(r)) + " '" +
                               swe::exprText(*receiverOf(call)) + "'";
                    } else if (n == "start" && r == swe::Role::Sequence) {
                        what = "starts sequence '" + swe::exprText(*receiverOf(call)) + "'";
                    } else {
                        return;
                    }
                    diags.addViolation(
                        "UVM-ROL-001", "DriverGeneratesStimulus",
                        "Driver '" + std::string(cls->name) + "' " + what + " in " +
                            std::string(m->name) + "(). Stimulus generated inside the driver cannot "
                            "be constrained or replaced by tests; move it into a sequence and let the "
                            "driver only drive the items it receives.",
                        core::Severity::Warning, swe::locOf(call));
                });
            }
        }
    }
};

// UVM-ROL-002 -----------------------------------------------------------------------------

class CoverageOutsideObservers : public Rule {
public:
    std::string_view getId() const override { return "UVM-ROL-002"; }
    std::string_view getName() const override { return "CoverageOutsideObservers"; }
    std::string_view getDescription() const override {
        return "Flags covergroups declared in drivers and sequences. They sample what was intended, "
               "not what reached the DUT, and disappear when the agent is passive; collect coverage "
               "in a monitor or subscriber (MG-3.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            swe::Role role = swe::classRole(*cls);
            if (role != swe::Role::Driver && role != swe::Role::Sequence) continue;
            for (const auto* p : meth::ownProperties(*cls)) {
                if (!p->getType().getCanonicalType().isCovergroup()) continue;
                diags.addViolation(
                    "UVM-ROL-002", "CoverageOutsideObservers",
                    std::string(swe::roleName(role)) + " '" + std::string(cls->name) +
                        "' holds covergroup '" + std::string(p->name) + "'. Coverage sampled here "
                        "records stimulus that was requested, even if the DUT never saw it, and is "
                        "lost when the agent runs passive. Sample it in a monitor or a coverage "
                        "subscriber fed from the monitor's analysis port.",
                    core::Severity::Warning, p->location);
            }
        }
    }
};

// UVM-ROL-003 -----------------------------------------------------------------------------

namespace {

bool isCheckerClass(const ClassType& cls) {
    if (swe::isLibrarySymbol(cls)) return false;
    if (swe::classRole(cls) == swe::Role::Scoreboard) return true;
    std::string n = lower(meth::plainName(cls));
    return n.find("scoreboard") != std::string::npos || n.find("predictor") != std::string::npos ||
           n.find("ref_model") != std::string::npos || n.find("refmodel") != std::string::npos ||
           n.find("reference_model") != std::string::npos;
}

} // namespace

class DutCheckingInAgent : public Rule {
public:
    std::string_view getId() const override { return "UVM-ROL-003"; }
    std::string_view getName() const override { return "DutCheckingInAgent"; }
    std::string_view getDescription() const override {
        return "Flags agents that hold a scoreboard, predictor or reference model. Agents carry "
               "protocol knowledge only; DUT-specific checking belongs in the env so the agent can be "
               "reused with other DUTs (MG-3.2).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Agent) continue;
            for (const auto* p : meth::ownProperties(*cls)) {
                const ClassType* t = meth::classOfType(p->getType());
                if (!t || !isCheckerClass(*t)) continue;
                diags.addViolation(
                    "UVM-ROL-003", "DutCheckingInAgent",
                    "Agent '" + std::string(cls->name) + "' contains '" + std::string(p->name) + "' of "
                        "type '" + std::string(meth::plainName(*t)) + "'. Expected-value models and "
                        "scoreboards are specific to one DUT, so the agent cannot be reused for another "
                        "one. Build them in the env and connect them to the agent's analysis port.",
                    core::Severity::Warning, p->location);
            }
        }
    }
};

// UVM-ROL-004 -----------------------------------------------------------------------------

class TestBuildsLeafComponents : public Rule {
public:
    std::string_view getId() const override { return "UVM-ROL-004"; }
    std::string_view getName() const override { return "TestBuildsLeafComponents"; }
    std::string_view getDescription() const override {
        return "Flags tests that create drivers, monitors, sequencers, agents, scoreboards or "
               "subscribers directly. Tests configure and select; the env builds, so every test "
               "shares one structure (facade, MG-3.3).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Test) continue;
            for (const auto& c : meth::creationsIn(*cls)) {
                swe::Role r = swe::classRole(*c.type);
                switch (r) {
                    case swe::Role::Driver: case swe::Role::Monitor: case swe::Role::Sequencer:
                    case swe::Role::Agent: case swe::Role::Scoreboard: case swe::Role::Subscriber:
                        break;
                    default:
                        continue;
                }
                diags.addViolation(
                    "UVM-ROL-004", "TestBuildsLeafComponents",
                    "Test '" + std::string(cls->name) + "' creates " + swe::roleName(r) + " '" +
                        std::string(meth::plainName(*c.type)) + "' itself. Each test that builds its "
                        "own components drifts from the others; create an env that builds them, and "
                        "change it from the test through its config object or factory overrides.",
                    core::Severity::Warning, c.loc);
            }
        }
    }
};

// UVM-ROL-005 -----------------------------------------------------------------------------

class MonitorDependsOnActivePath : public Rule {
public:
    std::string_view getId() const override { return "UVM-ROL-005"; }
    std::string_view getName() const override { return "MonitorDependsOnActivePath"; }
    std::string_view getDescription() const override {
        return "Flags monitors that hold a handle to a driver, sequencer or sequence item port. A "
               "monitor must work on its own, observing only the interface, or it breaks in passive "
               "agents (MG-3.1).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) != swe::Role::Monitor) continue;
            for (const auto* p : meth::ownProperties(*cls)) {
                const ClassType* t = meth::classOfType(p->getType());
                if (!t) continue;
                swe::Role r = swe::classRole(*t);
                bool itemPort = meth::plainName(*t).starts_with("uvm_seq_item_pull");
                if (r != swe::Role::Driver && r != swe::Role::Sequencer && !itemPort) continue;
                diags.addViolation(
                    "UVM-ROL-005", "MonitorDependsOnActivePath",
                    "Monitor '" + std::string(cls->name) + "' holds '" + std::string(p->name) +
                        "' of type '" + std::string(meth::plainName(*t)) + "'. Passive agents have no "
                        "driver or sequencer, so the monitor fails there; observe the interface only "
                        "and publish what you see on an analysis port.",
                    core::Severity::Warning, p->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDriverGeneratesStimulus() { return std::make_unique<methodology::DriverGeneratesStimulus>(); }
std::unique_ptr<Rule> createCoverageOutsideObservers() { return std::make_unique<methodology::CoverageOutsideObservers>(); }
std::unique_ptr<Rule> createDutCheckingInAgent() { return std::make_unique<methodology::DutCheckingInAgent>(); }
std::unique_ptr<Rule> createTestBuildsLeafComponents() { return std::make_unique<methodology::TestBuildsLeafComponents>(); }
std::unique_ptr<Rule> createMonitorDependsOnActivePath() { return std::make_unique<methodology::MonitorDependsOnActivePath>(); }
}
