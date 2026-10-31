#include "uvmcheck/rules/BuiltinRules.h"

#include <algorithm>

#include "uvmcheck/rules/RuleRegistry.h"

namespace uvmcheck::rules {

std::vector<RuleFactory>& ruleFactories() {
    static std::vector<RuleFactory> factories;
    return factories;
}

void RuleRegistry::sortById() {
    std::stable_sort(rules.begin(), rules.end(),
                     [](const auto& a, const auto& b) { return a->getId() < b->getId(); });
}

void registerBuiltinRules() {
    auto& reg = RuleRegistry::instance();
    if (!reg.getRules().empty()) return;
    for (auto factory : ruleFactories()) reg.registerRule(factory());
    reg.sortById();
}

} // namespace uvmcheck::rules
