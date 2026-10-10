*source: dragon book*

# Terms

* **Lexer** reads stream of **characters**
* group/sequences of characters are **lexemes**
* a **symbol table** holds all identifiers, and their relations (including types)
* a **syntax analyser** produces a **syntax tree** which is a machine-readable representation of the source code
    * question: this should also annotate the types of identifiers. But it isn't just a representation of the source
      code then. Where am I doing that if there are implicit types
* a **semantic analyser** produces a **syntax tree** which does type checking and implicit conversions

# Conventions

* punctuation tokens have no special name: the token '=' has the name '='
