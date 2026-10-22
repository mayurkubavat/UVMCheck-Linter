#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/ConditionalStatements.h"

#include <map>

namespace uvmcheck::rules::design {

using namespace slang::ast;

namespace {

struct CaseFinder : public ASTVisitor<CaseFinder, VisitFlags::AllGood> {
    std::map<const Type*, const CaseStatement*> firstByEnum;
    void handle(const CaseStatement& c) {
        const Type& t = c.expr.type->getCanonicalType();
        if (t.kind == SymbolKind::EnumType) firstByEnum.emplace(&t, &c);
        visitDefault(c);
    }
};

} // namespace

class RepeatedEnumSwitch : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-009"; }
    std::string_view getName() const override { return "RepeatedEnumSwitch"; }
    std::string_view getDescription() const override {
        return "Flags an enum that is switched on with case in three or more classes, a sign that "
               "behavior per kind belongs in a polymorphic item or policy class (Fowler 'repeated "
               "switches'). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t minClasses = swe::ruleOption("SV-DSN-009", "min_classes", 3);
        struct Site { const ClassType* cls; const CaseStatement* stmt; };
        std::map<const Type*, std::vector<Site>> sites;
        std::vector<const Type*> order;
        for (const auto* cls : swe::userClasses(model)) {
            CaseFinder f;
            for (const auto* m : swe::classMethods(*cls)) m->getBody().visit(f);
            for (const auto& [type, stmt] : f.firstByEnum) {
                auto& v = sites[type];
                if (v.empty()) order.push_back(type);
                v.push_back({cls, stmt});
            }
        }
        for (const auto* type : order) {
            const auto& v = sites[type];
            if (static_cast<int64_t>(v.size()) < minClasses) continue;
            std::string classes;
            for (const auto& s : v) {
                if (!classes.empty()) classes += ", ";
                classes += std::string(s.cls->name);
            }
            std::string enumName = type->name.empty() ? type->toString() : std::string(type->name);
            for (const auto& s : v) {
                diags.addViolation(
                    "SV-DSN-009", "RepeatedEnumSwitch",
                    "'" + std::string(s.cls->name) + "' switches on enum '" + enumName + "', as do " +
                        std::to_string(v.size() - 1) + " other classes (" + classes + "). Every new "
                        "value needs an edit in each of them; consider moving the per-kind behavior "
                        "into the item or a policy class.",
                    core::Severity::Info, swe::locOf(*s.stmt));
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRepeatedEnumSwitch() {
    return std::make_unique<design::RepeatedEnumSwitch>();
}
}
