# Una casa 3D con texture (sperimentale)

Fuori dalla papera: una casa 3D con i colori della camera della testa, da
ciò che la papera manda già — i frame di mediad, `frames.camera` /
`frames.tof` e l'IMU di robotd, le distanze 8x8 di tofd e la posa sulla
mappa di quack-navd. Inglese: [README.md](README.md).

```sh
python3 scripts/map3d/record.py $STATE rec/ 300 [--sim-port 7872]   # accanto a una papera che esplora
python3 scripts/map3d/build.py rec/ out/ --voxel 0.02               # iso_*.png, top.png, cloud.ply
python3 scripts/map3d/accuracy.py out/ rec/ <casa>.truth.json       # solo il gemello
```

- `record.py` tiene un frame ogni mezzo secondo mentre la papera è ferma.
- `build.py`: Depth Anything V2 **Small** (Apache-2.0; Base e Large sono
  non commerciali, non usati) dà a ogni frame una profondità relativa; i
  ritorni del sensore di profondità visti dalla camera la portano in
  metri, frame per frame; i frame si fondono in un TSDF sul sistema della
  mappa con i colori della camera, e si disegnano in isometrica. Servono
  torch, transformers, Pillow.
- Prima prova (2026-10-14, casa_arredata, velstand, 5 minuti di
  esplorazione, 42 % della casa mappato): 248 frame fusi su 296; contro la
  verità del gemello, il pavimento entro 1 cm (mediana), pareti e mobili
  2,3 cm di mediana, il 96 % entro 10 cm.
- Solo nel simulatore la camera della testa guardava all'indietro: la
  camera MJCF del modello di microduck_rl, non quella della papera
  (`frames.camera` di robotd guarda avanti, accanto al sensore di
  profondità). La correzione è mezzo giro attorno all'x della camera,
  esattamente il passaggio dal sistema camera di OpenCV (z avanti, y in
  basso) a quello di MuJoCo (guarda lungo -z, y in alto): il `quat` del
  modello è stato quasi certamente scritto per il primo.
  `viewer/body_with_map.py` la corregge (`TWIN_CAMERA_BACKWARD=1` tiene
  quella del modello).
- Sulla papera: gli assi della camera rispetto a `frames.camera` e gli
  intrinseci (`media.video`) vanno verificati con la stessa
  sovrapposizione dei ritorni del sensore sull'immagine.
