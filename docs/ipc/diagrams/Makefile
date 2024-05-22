PUMLS = $(shell find . -type f -name "*.puml")
PNGS  = $(PUMLS:.puml=.png)
PUML_VER=1.2023.2

.PHONY: all
all: diagrams

.PHONY: diagrams
diagrams: $(PNGS)

plantuml.jar:
	wget -O $@ https://github.com/plantuml/plantuml/releases/download/v$(PUML_VER)/plantuml-$(PUML_VER).jar --no-check-certificate --quiet

%.png: plantuml.jar %.puml
	@# Using pipelining to preserve file names.
	cat $*.puml | java -jar plantuml.jar -pipe > $*.png
