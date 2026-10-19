#pragma once

#include <vector>
#include <memory>
#include <string_view>
#include "uvmcheck/rules/Rule.h"

namespace uvmcheck::rules {

class RuleRegistry {
public:
    static RuleRegistry& instance() {
        static RuleRegistry reg;
        return reg;
    }

    void registerRule(std::unique_ptr<Rule> rule) {
        rules.push_back(std::move(rule));
    }

    const std::vector<std::unique_ptr<Rule>>& getRules() const {
        return rules;
    }

private:
    RuleRegistry() = default;
    std::vector<std::unique_ptr<Rule>> rules;
};

template <typename T>
struct RuleRegistrar {
    RuleRegistrar() {
        RuleRegistry::instance().registerRule(std::make_unique<T>());
    }
};

#define UVMCHECK_REGISTER_RULE(ClassName) \
    static ::uvmcheck::rules::RuleRegistrar<ClassName> s_registrar_##ClassName;

} // namespace uvmcheck::rules
