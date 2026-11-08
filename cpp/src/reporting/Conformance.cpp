#include "uvmcheck/reporting/Conformance.h"

#include <iomanip>
#include <ostream>
#include <string>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::reporting {

std::vector<GuidelineResult> computeConformance(const std::vector<rules::Rule*>& activeRules,
                                                const std::vector<core::Violation>& violations) {
    std::unordered_set<std::string_view> active;
    for (const auto* r : activeRules) active.insert(r->getId());

    std::unordered_map<std::string, size_t> perRule;
    for (const auto& v : violations) perRule[v.ruleId]++;

    std::vector<GuidelineResult> out;
    for (const auto& g : rules::methodologyGuidelines()) {
        GuidelineResult res;
        res.guideline = &g;
        for (auto id : rules::rulesForGuideline(g.id)) {
            if (!active.count(id)) continue;
            res.rules.push_back(id);
            auto it = perRule.find(std::string(id));
            if (it != perRule.end()) res.findings += it->second;
        }
        if (!res.rules.empty()) out.push_back(std::move(res));
    }
    return out;
}

void printConformance(std::ostream& os, const std::vector<GuidelineResult>& results) {
    if (results.empty()) return;
    os << "\nMethodology guide conformance\n";
    std::string_view chapter;
    size_t chapterPass = 0, chapterTotal = 0;
    auto flush = [&] {
        if (chapterTotal) {
            os << "  " << chapter << ": " << chapterPass << " of " << chapterTotal << " guidelines pass\n";
        }
    };
    for (const auto& r : results) {
        if (r.guideline->chapter != chapter) {
            flush();
            chapter = r.guideline->chapter;
            chapterPass = chapterTotal = 0;
            os << "  " << chapter << " " << r.guideline->chapterTitle << "\n";
        }
        chapterTotal++;
        if (r.passed()) chapterPass++;
        os << "    " << std::left << std::setw(8) << r.guideline->id << std::setw(8)
           << rules::levelName(r.guideline->level) << (r.passed() ? "pass" : "FAIL");
        if (!r.passed()) os << "  " << r.findings << " finding" << (r.findings == 1 ? "" : "s");
        os << "\n";
    }
    flush();
}

} // namespace uvmcheck::reporting
