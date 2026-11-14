#!/usr/bin/env bash
# Mise à niveau de la base PhotoCraft de DTF Craft (fusion à trois voies).
#
# L'historique de DTF Craft n'est pas relié à celui de PhotoCraft (import en un commit). Ce script
# recrée l'ancêtre commun à partir de PHOTOCRAFT-BASE.toml, sans pousser l'historique de PhotoCraft
# dans le dépôt DTF Craft :
#
#   B = arbre de la base PhotoCraft            (commit local, sans parent)
#   D = arbre de DTF Craft (HEAD)    parent B
#   T = arbre de la version PhotoCraft cible   parent B
#
# Git fusionne alors T dans D en prenant B comme ancêtre commun : seules les modifications DTF qui
# touchent les mêmes lignes que PhotoCraft donnent des conflits.
#
# Usage, depuis la racine du dépôt, arbre de travail propre, sur main (ou une branche de main) :
#
#   scripts/dtf-sync-photocraft.sh start v0.23.1   # crée sync/photocraft-v0.23.1 et lance la fusion
#   (résoudre les conflits, `git add` les fichiers, compiler, tester)
#   scripts/dtf-sync-photocraft.sh finish          # crée maj/photocraft-v0.23.1 : un seul commit
#                                                  # au-dessus de la branche de départ, à pousser
#   scripts/dtf-sync-photocraft.sh abort           # abandonne et revient à la branche de départ
#
# La branche sync/… a un historique artificiel : ne pas la pousser. Seule maj/… est poussée.
set -euo pipefail

BASE_FILE=PHOTOCRAFT-BASE.toml
STATE_DIR=$(git rev-parse --git-dir)/dtf-sync
KEEP_WORKFLOW=.github/workflows/dtf-craft-windows.yml

die() { echo "Erreur : $*" >&2; exit 1; }

toml_value() { # toml_value <clé> : valeur de la section [photocraft]
    sed -n "/^\[photocraft\]/,/^\[/{s/^$1 *= *\"\(.*\)\".*/\1/p}" "$BASE_FILE" | head -n 1
}

# Fichiers PhotoCraft à garder hors de DTF Craft : la liste `removed` et tout workflow GitHub
# autre que celui de DTF Craft (PhotoCraft en ajoute régulièrement).
drop_removed_files() {
    local f
    sed -n '/^removed *= *\[/,/\]/{s/^ *"\(.*\)",\{0,1\} *$/\1/p}' "$BASE_FILE" | while read -r f; do
        [ -n "$f" ] && git rm -q -f --ignore-unmatch -- "$f"
    done
    git ls-files -- .github/workflows | grep -vxF "$KEEP_WORKFLOW" | while read -r f; do
        git rm -q -f -- "$f"
    done
}

cmd_start() {
    local target=${1:-}
    [ -n "$target" ] || die "indiquez la version PhotoCraft cible (ex. v0.23.1)"
    [ -f "$BASE_FILE" ] || die "$BASE_FILE introuvable : lancez le script depuis la racine du dépôt"
    [ -z "$(git status --porcelain)" ] || die "l'arbre de travail n'est pas propre"
    [ ! -d "$STATE_DIR" ] || die "une mise à niveau est déjà en cours (finish ou abort)"

    local repo base start_branch dtf
    repo=$(toml_value repository)
    base=$(toml_value commit)
    [ -n "$repo" ] && [ -n "$base" ] || die "$BASE_FILE incomplet"
    start_branch=$(git symbolic-ref --short HEAD) || die "HEAD détaché : placez-vous sur une branche"
    dtf=$(git rev-parse HEAD)

    echo "Récupération de PhotoCraft (base ${base:0:9} et $target)…"
    git fetch -q --no-tags --depth=1 "$repo" "$base"
    git fetch -q --no-tags --depth=1 "$repo" "+refs/tags/$target:refs/dtf-sync/target" 2>/dev/null \
        || git fetch -q --no-tags --depth=1 "$repo" "+$target:refs/dtf-sync/target" \
        || die "version PhotoCraft « $target » introuvable"

    local target_commit B D T
    target_commit=$(git rev-parse 'refs/dtf-sync/target^{commit}')
    B=$(git commit-tree "$base^{tree}" -m "PhotoCraft (base ${base:0:9})")
    D=$(git commit-tree "$dtf^{tree}" -p "$B" -m "DTF Craft ($start_branch)")
    T=$(git commit-tree "$target_commit^{tree}" -p "$B" -m "PhotoCraft $target")

    mkdir -p "$STATE_DIR"
    printf '%s\n' "$start_branch" > "$STATE_DIR/start_branch"
    printf '%s\n' "$dtf" > "$STATE_DIR/dtf"
    printf '%s\n' "$target" > "$STATE_DIR/target"
    printf '%s\n' "$target_commit" > "$STATE_DIR/target_commit"

    git checkout -q -B "sync/photocraft-$target" "$D"
    git merge --no-ff --no-commit -m "Fusion PhotoCraft $target" "$T" >/dev/null 2>&1 || true
    drop_removed_files

    local conflicts
    conflicts=$(git diff --name-only --diff-filter=U)
    if [ -n "$conflicts" ]; then
        echo "Fusion de PhotoCraft $target : $(printf '%s\n' "$conflicts" | wc -l | tr -d ' ') fichier(s) en conflit :"
        printf '  %s\n' $conflicts
        echo "Résolvez-les, faites git add, puis : scripts/dtf-sync-photocraft.sh finish"
    else
        echo "Fusion de PhotoCraft $target sans conflit. Compilez et testez, puis : scripts/dtf-sync-photocraft.sh finish"
    fi
}

cmd_finish() {
    [ -d "$STATE_DIR" ] || die "aucune mise à niveau en cours"
    local start_branch dtf target target_commit
    start_branch=$(cat "$STATE_DIR/start_branch")
    dtf=$(cat "$STATE_DIR/dtf")
    target=$(cat "$STATE_DIR/target")
    target_commit=$(cat "$STATE_DIR/target_commit")
    [ -z "$(git diff --name-only --diff-filter=U)" ] || die "il reste des conflits non résolus"

    # La nouvelle base PhotoCraft devient la référence des prochaines mises à niveau.
    sed -i.bak -e "/^\[photocraft\]/,/^\[/{s/^commit *= *\".*\"/commit = \"$target_commit\"/;s/^version *= *\".*\"/version = \"${target#v}\"/}" "$BASE_FILE"
    rm -f "$BASE_FILE.bak"
    git add -A
    git diff --cached --quiet || git commit -q --no-verify -m "Fusion PhotoCraft $target"

    local tree result
    tree=$(git rev-parse 'HEAD^{tree}')
    result=$(git commit-tree "$tree" -p "$dtf" -m "Mise à niveau PhotoCraft ${target}

Base PhotoCraft : $target ($target_commit), fusion à trois voies par scripts/dtf-sync-photocraft.sh.")
    git checkout -q -B "maj/photocraft-$target" "$result"
    git branch -q -D "sync/photocraft-$target"
    git update-ref -d refs/dtf-sync/target
    rm -rf "$STATE_DIR"
    echo "Branche maj/photocraft-$target créée au-dessus de $start_branch : à pousser et à relire avant fusion dans main."
}

cmd_abort() {
    [ -d "$STATE_DIR" ] || die "aucune mise à niveau en cours"
    local start_branch target
    start_branch=$(cat "$STATE_DIR/start_branch")
    target=$(cat "$STATE_DIR/target")
    git merge --abort 2>/dev/null || git reset -q --hard
    git checkout -q -f "$start_branch"
    git branch -q -D "sync/photocraft-$target" 2>/dev/null || true
    git update-ref -d refs/dtf-sync/target 2>/dev/null || true
    rm -rf "$STATE_DIR"
    echo "Mise à niveau abandonnée, retour sur $start_branch."
}

case "${1:-}" in
    start) shift; cmd_start "$@" ;;
    finish) cmd_finish ;;
    abort) cmd_abort ;;
    *) sed -n '2,24p' "$0"; exit 1 ;;
esac
