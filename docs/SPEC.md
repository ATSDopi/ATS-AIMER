# ATS-AIMER — Cahier des charges (spec de travail)

Produit : enregistreur « boîte noire » post-match de télémétrie mécanique pour
VALORANT (puis autres tac-FPS). Croise la vidéo du match avec les entrées
matérielles brutes (souris + clavier) et analyse chaque duel une fois la partie
terminée.

## Principes non négociables

- **Jamais** de lecture mémoire, d'injection, d'input synthétique, ni d'analyse
  en temps réel pendant la partie. Toute l'analyse est en différé.
- La sensibilité n'est jamais changée par l'outil : il recommande, l'utilisateur
  applique en jeu.
- Pas de promesse de « sensi parfaite » : on produit un **point d'équilibre
  statistique** + une **Zone Verte** de tolérance + des règles
  d'ajustement ±5 %.

## Modules

### 1. Boîte noire (recorder/) — Rust, windows-rs

- Capture écran passive via Windows Graphics Capture API (ou FFmpeg NVENC) —
  encodage GPU, buffer circulaire ou par match.
- Raw Input (`WM_INPUT`, `RIDEV_INPUTSINK` pour capter en arrière-plan) :
  Δx/Δy souris horodatés (1000–8000 Hz), clics, touches de déplacement
  (Z/Q/S/D, saut, walk).
- Sortie par session : `video.mp4` + `input.parquet` (ou .jsonl) +
  `meta.json` (résolution, FOV déclaré, sensi déclarée, DPI, version du jeu).

### 2. Découpeur de duels (analysis/) — Python + OpenCV

- Détection des engagements : killfeed (bannière « userMark » jaune ~4 s),
  perte de PV, flash de dégât, bursts de clics souris comme fallback.
- Extraction de micro-clips (~3 s) centrés sur chaque duel.

### 3. Moteur biomécanique (analysis/) — Python

- Détection de la silhouette ennemie via la couleur de surbrillance
  (rouge/violet/jaune — segmentation HSV ; YOLO léger fine-tuné en fallback).
- Distance viseur→tête par frame + fusion avec la courbe physique souris et
  l'état clavier.
- Métriques par duel : erreur de 1er flick (signée), temps de
  micro-ajustement, click jitter (20 ms), taux de tir en mouvement
  (< 80 ms après relâchement = exclu), indice de constance (σ).
- Radar directionnel 360° : sur/sous-flick par secteur angulaire
  (diagnostic posture/poignet, exercices ciblés — **pas** de sensi
  directionnelle, cf. piège anti-cheat).

### 4. Comparateur & coach (app/) — Tauri + React

- Protocole « Blind / Multi-Sensi » : 5–7 jours (≈15 DM) à sensi variant
  autour de la base (−15/−7/0/+7/+15 %), valeur affichée masquée.
- Sortie : point d'équilibre statistique, Zone Verte (top 95 %), règle de
  micro-ajustement ±5 % pilotée par les données.
- Check-up moteur 90 s pré-session (état de chauffe/tension) — **jamais** de
  « sensi du jour ».
- Conseils actionnables issus de la fusion vidéo+inputs (synchro
  counter-strafe/clic, hauteur de viseur vs tête, drift fatigue, quadrant
  faible du radar).

## Onboarding

3 champs obligatoires : jeu cible, sensi actuelle, DPI déclaré. Résolution et
ratio auto-détectés. Le reste (tapis, grip, patins) = métadonnées optionnelles.
La sensi de base sert de **groupe témoin** comparatif, jamais de borne de
recherche.

## Hors périmètre V1

- Coaching crosshair placement in-game (dépend des vraies cartes).
- Sensi adaptative en jeu (driver d'interception = problème anti-cheat).
- Analyse temps réel, overlay in-match, tout avantage pendant la partie.

## Références

- Spec source : `aim.pdf` (critique Gemini + refonte).
- Concurrents : Aimlabs Mouselabs SF 2.0, Oblivity, PSA method, Insights
  Capture, game-sensitivity-coach (agent skill le plus proche — voir README).
