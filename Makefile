COMPONENTS = mithril-common mithril-stm mithril-aggregator mithril-client mithril-client-cli mithril-signer \
			internal/mithril-persistence internal/mithril-doc-derive internal/mithril-doc internal/mithril-build-script \
			demo/protocol-demo mithril-test-lab/mithril-end-to-end

LINT_COMPONENTS = mithril-explorer mithril-client-wasm docs/website

GOALS := $(or $(MAKECMDGOALS),all)

.PHONY: $(GOALS) $(COMPONENTS) lint $(LINT_COMPONENTS)

all: $(COMPONENTS)

$(COMPONENTS):
	$(MAKE) -C $@ $(GOALS)

lint: lint-components

lint-components: $(LINT_COMPONENTS)

$(LINT_COMPONENTS):
	$(MAKE) -C $@ lint
