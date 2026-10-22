#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

class LowCohesion : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-003"; }
    std::string_view getName() const override { return "LowCohesion"; }
    std::string_view getDescription() const override {
        return "Flags classes whose methods split into two or more groups that share no fields and do "
               "not call each other (LCOM4 > 1): the class is really several classes.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t minGroups = swe::ruleOption("SV-DSN-003", "groups", 2);
        const int64_t minGroupSize = swe::ruleOption("SV-DSN-003", "min_group_size", 2);
        for (const auto& cm : metrics::classMetrics(compilation, model)) {
            swe::Role role = swe::classRole(*cm.cls);
            // Data classes: their accessors touch one field each by design.
            if (role == swe::Role::Config || role == swe::Role::Item || role == swe::Role::RegModel) continue;
            std::vector<const std::vector<std::string>*> groups;
            for (const auto& g : cm.lcomGroups) {
                if (static_cast<int64_t>(g.size()) >= minGroupSize) groups.push_back(&g);
            }
            if (static_cast<int64_t>(groups.size()) < minGroups) continue;
            std::string desc;
            for (size_t i = 0; i < groups.size(); i++) {
                if (i) desc += "; ";
                desc += "{";
                for (size_t j = 0; j < groups[i]->size(); j++) {
                    if (j) desc += ", ";
                    desc += (*groups[i])[j];
                }
                desc += "}";
            }
            diags.addViolation(
                "SV-DSN-003", "LowCohesion",
                std::string(swe::roleName(role)) + " '" + std::string(cm.cls->name) + "' has " +
                    std::to_string(groups.size()) + " groups of methods that share no data: " + desc +
                    ". Each group is a separate responsibility; consider moving them into their own classes.",
                core::Severity::Info, cm.cls->location);
        }
    }
};

class HighCoupling : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-004"; }
    std::string_view getName() const override { return "HighCoupling"; }
    std::string_view getDescription() const override {
        return "Flags classes other than envs and tests that reference more than 10 distinct user "
               "class types (CK coupling between objects, PMD CouplingBetweenObjects).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-DSN-004", "max_cbo", 10);
        for (const auto& cm : metrics::classMetrics(compilation, model)) {
            swe::Role role = swe::classRole(*cm.cls);
            if (role == swe::Role::Env || role == swe::Role::Test || role == swe::Role::RegModel) continue;
            if (cm.cbo <= limit) continue;
            std::string list;
            for (size_t i = 0; i < cm.coupledTo.size(); i++) {
                if (i) list += ", ";
                list += cm.coupledTo[i];
            }
            diags.addViolation(
                "SV-DSN-004", "HighCoupling",
                std::string(swe::roleName(role)) + " '" + std::string(cm.cls->name) + "' depends on " +
                    std::to_string(cm.cbo) + " classes (limit " + std::to_string(limit) + "): " + list +
                    ". A change to any of them can break it; depend on the item, the config and "
                    "interfaces, and reach the rest through TLM.",
                core::Severity::Info, cm.cls->location);
        }
    }
};

class DeepInheritance : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-005"; }
    std::string_view getName() const override { return "DeepInheritance"; }
    std::string_view getDescription() const override {
        return "Flags classes with more than 3 user-defined ancestors above the UVM base class "
               "(CK depth of inheritance tree, Pylint too-many-ancestors).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-DSN-005", "max_depth", 3);
        for (const auto& cm : metrics::classMetrics(compilation, model)) {
            if (cm.dit <= limit) continue;
            std::string chain(cm.cls->name);
            for (const auto& a : cm.ancestry) chain += " : " + a;
            std::string_view base = swe::uvmBaseName(*cm.cls);
            if (!base.empty()) chain += " : " + std::string(base);
            diags.addViolation(
                "SV-DSN-005", "DeepInheritance",
                "'" + std::string(cm.cls->name) + "' has " + std::to_string(cm.dit) +
                    " user-defined ancestors (limit " + std::to_string(limit) + "): " + chain +
                    ". It is hard to tell which override of each phase runs; prefer composition or "
                    "config objects over another level.",
                core::Severity::Info, cm.cls->location);
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLowCohesion() {
    return std::make_unique<design::LowCohesion>();
}
std::unique_ptr<Rule> createHighCoupling() {
    return std::make_unique<design::HighCoupling>();
}
std::unique_ptr<Rule> createDeepInheritance() {
    return std::make_unique<design::DeepInheritance>();
}
}
