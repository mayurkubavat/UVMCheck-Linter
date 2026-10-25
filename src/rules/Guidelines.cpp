#include "uvmcheck/rules/Guidelines.h"

#include <unordered_map>

namespace uvmcheck::rules {

namespace {

using L = GuidelineLevel;

constexpr std::string_view kLayering = "Layering and packages";
constexpr std::string_view kCoupling = "Coupling";
constexpr std::string_view kRoles = "Cohesion and component roles";
constexpr std::string_view kReuse = "Reusable verification components";
constexpr std::string_view kPatterns = "Design-pattern conformance";
constexpr std::string_view kConstructs = "Construct usage";
constexpr std::string_view kDuplication = "Duplication";
constexpr std::string_view kContracts = "Contracts and failure handling";
constexpr std::string_view kEvolvability = "Evolvability";

struct Entry {
    Guideline guideline;
    std::vector<std::string_view> rules;
};

// Keep in step with docs/methodology/. tests/test_guidelines.py checks that every rule
// listed here exists and that each chapter page lists the same rules.
const std::vector<Entry>& entries() {
    static const std::vector<Entry> table = {
        // MG-1 Layering and packages
        {{"MG-1.1", "MG-1", kLayering, L::Shall,
          "Dependencies point down the testbench stack: item, sequence, agent, env, test. Nothing "
          "lower knows about anything higher."},
         {"UVM-ARC-001", "SV-PKG-004"}},
        {{"MG-1.2", "MG-1", kLayering, L::Should,
          "One class per file, and the file is named after the class."},
         {"SV-PKG-001"}},
        {{"MG-1.3", "MG-1", kLayering, L::Shall,
          "Each file belongs to one package; declarations, imports and macros stay inside it and do "
          "not leak into $unit or other files."},
         {"UVM-ARC-003", "SV-MOD-001", "SV-MAC-001", "SV-STY-013", "SV-PKG-002", "SV-PKG-003"}},
        {{"MG-1.4", "MG-1", kLayering, L::May,
          "Packages that many others depend on stay small and stable."},
         {"SV-PKG-005"}},

        // MG-2 Coupling
        {{"MG-2.1", "MG-2", kCoupling, L::Shall,
          "Components talk through TLM ports, config objects and sequences; they do not hold "
          "handles to envs and tests or write each other's state."},
         {"UVM-CPL-002", "UVM-CPL-003", "UVM-ARC-002", "UVM-SEQ-003", "UVM-SEQ-011", "UVM-PHS-010",
          "SV-ENC-001", "SV-DSN-004"}},
        {{"MG-2.2", "MG-2", kCoupling, L::Shall,
          "Class code never names a module hierarchy; virtual interfaces arrive through "
          "configuration."},
         {"UVM-CPL-004", "SV-OOP-008"}},
        {{"MG-2.3", "MG-2", kCoupling, L::Should,
          "Shared state is passed down at build time, not kept in globals, statics, the command "
          "line or run-time hierarchy lookups."},
         {"SV-DSN-008", "SV-OOP-007", "UVM-CFG-003", "UVM-CFG-005", "UVM-CFG-006", "UVM-ARC-004",
          "UVM-CFG-007", "UVM-CFG-009"}},
        {{"MG-2.4", "MG-2", kCoupling, L::Should,
          "Names and positions that tie code together (config keys, value types, argument order) "
          "match on both sides and are checked, not left to convention."},
         {"UVM-CFG-001", "UVM-DAG-001", "SV-RDB-001", "SV-BUG-007", "UVM-CFG-008", "SV-CPL-001"}},

        // MG-3 Cohesion and component roles
        {{"MG-3.1", "MG-3", kRoles, L::Shall,
          "Each UVM role does one job: sequences generate, drivers drive, monitors observe, "
          "scoreboards check, subscribers cover."},
         {"UVM-ROL-001", "UVM-ROL-002", "UVM-ROL-005", "UVM-SEQ-001", "UVM-VIF-DIR", "UVM-OBS-001",
          "UVM-TLM-005", "UVM-ALS-001", "SV-DSN-002", "SV-DSN-003", "SV-MET-006"}},
        {{"MG-3.2", "MG-3", kRoles, L::Should,
          "Agents carry protocol knowledge only; DUT-specific checking lives in the env."},
         {"UVM-ROL-003"}},
        {{"MG-3.3", "MG-3", kRoles, L::Should,
          "Tests configure and select; envs build."},
         {"UVM-ROL-004"}},

        // MG-4 Reusable verification components
        {{"MG-4.1", "MG-4", kReuse, L::Shall,
          "Every agent works in both active and passive mode."},
         {"UVM-RUS-001"}},
        {{"MG-4.2", "MG-4", kReuse, L::Shall,
          "Agent and env behaviour is controlled by a config object, not by literals or scattered "
          "config_db fields."},
         {"UVM-RUS-002", "UVM-RUS-003"}},
        {{"MG-4.3", "MG-4", kReuse, L::Should,
          "Overrides and lookups use types, not strings; config scopes are relative; nothing names "
          "a host path or a vendor-only call."},
         {"UVM-RUS-004", "UVM-RUS-005", "PORT-PTH-001", "PORT-PRP-001"}},
        {{"MG-4.4", "MG-4", kReuse, L::Should,
          "Sequence items carry their full object contract: copy, compare and print."},
         {"UVM-RUS-006", "SV-BUG-004", "UVM-SEQ-008"}},
        {{"MG-4.5", "MG-4", kReuse, L::Shall,
          "Every reusable class is registered with the factory and created through it."},
         {"UVM-FAC-001", "UVM-FAC-002", "UVM-FAC-003", "UVM-FAC-004", "UVM-STR-005", "UVM-SEQ-010"}},

        // MG-5 Design-pattern conformance
        {{"MG-5.1", "MG-5", kPatterns, L::Shall,
          "When a pattern's mechanism is used, all of its parts are present: callbacks are "
          "registered, adapters are complete, ports and sequencers are wired."},
         {"UVM-PAT-001", "RAL-ADP-001", "RAL-WIRE-001", "UVM-VSEQ-001", "UVM-TLM-001", "UVM-TLM-004"}},
        {{"MG-5.2", "MG-5", kPatterns, L::Should,
          "Behaviour varies through overrides, hooks and callbacks, not type switches or edits to "
          "the base class; overrides keep the base contract."},
         {"SV-DSN-001", "SV-DSN-009", "SV-OCP-001", "SV-DSN-006", "SV-DSN-007", "SV-OOP-001",
          "SV-OOP-009", "SV-OOP-015", "SV-LSP-001", "SV-RDB-004"}},
        {{"MG-5.3", "MG-5", kPatterns, L::Should,
          "No hand-rolled singletons; shared services go through the config or resource database."},
         {}},

        // MG-6 Construct usage
        {{"MG-6.1", "MG-6", kConstructs, L::Shall,
          "Prefer typed language constructs to the preprocessor and to code-generating macros."},
         {"UVM-BAN-001", "UVM-BAN-002", "SV-CNS-001", "SV-CNS-002", "SV-CNS-003"}},
        {{"MG-6.2", "MG-6", kConstructs, L::Should,
          "Model domain values with types: enums instead of strings, packed structs instead of "
          "hand slicing."},
         {"SV-CNS-004", "SV-CNS-005"}},
        {{"MG-6.3", "MG-6", kConstructs, L::Shall,
          "Use current, explicit language and UVM constructs; deprecated and legacy forms are not "
          "used."},
         {"SV-DEP-001", "SV-DEP-002", "SV-DEP-003", "SV-DEP-004", "SV-DEP-005", "SV-DEP-006",
          "UVM-DEP-001", "UVM-DEP-002", "UVM-DEP-003", "UVM-DEP-004", "SV-BAN-001", "SV-COR-009",
          "SV-CNS-006", "SV-CNS-007", "SV-CNS-008"}},

        // MG-7 Duplication
        {{"MG-7.1", "MG-7", kDuplication, L::Should,
          "Copy-paste is refactored into a shared base, helper or parameterized class by the third "
          "copy."},
         {"SV-DUP-001", "SV-DUP-002", "SV-DUP-003", "SV-DUP-004", "SV-RND-011"}},
        {{"MG-7.2", "MG-7", kDuplication, L::May,
          "Near-identical agents become one parameterized agent."},
         {"SV-DUP-005"}},

        // MG-8 Contracts and failure handling
        {{"MG-8.1", "MG-8", kContracts, L::Shall,
          "Every check that can fail reports through UVM at error severity, so failures are "
          "counted and never hidden."},
         {"UVM-REP-007", "UVM-REP-008", "SV-ERR-001", "SV-ERR-002", "SV-ERR-003", "SV-OOP-002",
          "SV-RND-001", "UVM-CFG-004", "UVM-TST-001", "UVM-REP-006", "UVM-SCB-001", "UVM-SCB-002",
          "UVM-SEQ-006", "UVM-TLM-002", "SV-IPC-001", "SV-SYS-001", "SVA-ACT-001", "UVM-LOG-002"}},
        {{"MG-8.2", "MG-8", kContracts, L::Should,
          "Reusable interfaces carry their own protocol assertions."},
         {}},

        // MG-9 Evolvability
        {{"MG-9.1", "MG-9", kEvolvability, L::Should,
          "Routines and classes stay within complexity and size budgets, and dead or commented-out "
          "code is removed."},
         {"SV-MET-001", "SV-MET-002", "SV-MET-003", "SV-MET-004", "SV-MET-005", "SV-SML-004",
          "SV-STY-009", "SV-RDB-002", "SV-RDB-003", "SV-RDB-005", "SV-API-001", "SV-DED-001",
          "SV-DED-005", "SV-DED-006", "SV-DED-011", "SV-DED-012", "SV-DED-013"}},
        {{"MG-9.2", "MG-9", kEvolvability, L::May,
          "Complex code that changes often is refactored first."},
         {"SV-EVO-001", "SV-EVO-002"}},
    };
    return table;
}

} // namespace

const char* levelName(GuidelineLevel level) {
    switch (level) {
        case GuidelineLevel::Shall: return "Shall";
        case GuidelineLevel::Should: return "Should";
        case GuidelineLevel::May: return "May";
    }
    return "Should";
}

const std::vector<Guideline>& methodologyGuidelines() {
    static const std::vector<Guideline> list = [] {
        std::vector<Guideline> out;
        for (const auto& e : entries()) out.push_back(e.guideline);
        return out;
    }();
    return list;
}

const Guideline* findGuideline(std::string_view id) {
    for (const auto& e : entries()) {
        if (e.guideline.id == id) return &e.guideline;
    }
    return nullptr;
}

const std::vector<std::string_view>& rulesForGuideline(std::string_view guidelineId) {
    static const std::vector<std::string_view> none;
    for (const auto& e : entries()) {
        if (e.guideline.id == guidelineId) return e.rules;
    }
    return none;
}

std::vector<std::string_view> guidelinesForRule(std::string_view ruleId) {
    static const auto index = [] {
        std::unordered_map<std::string_view, std::vector<std::string_view>> m;
        for (const auto& e : entries()) {
            for (auto r : e.rules) m[r].push_back(e.guideline.id);
        }
        return m;
    }();
    auto it = index.find(ruleId);
    return it == index.end() ? std::vector<std::string_view>{} : it->second;
}

bool guidelineMatches(std::string_view selector, std::string_view guidelineId) {
    if (selector == guidelineId) return true;
    // "MG-4" selects MG-4.1, MG-4.2 ... but not MG-40.1.
    return guidelineId.size() > selector.size() && guidelineId.starts_with(selector) &&
           guidelineId[selector.size()] == '.';
}

} // namespace uvmcheck::rules
