# Una casa 3D con texture (sperimentale)

Fuori dalla papera: una casa 3D con i colori della camera della testa, da
ciò che la papera manda già — i frame di mediad, `frames.camera` /
`frames.tof` e l'IMU di robotd, le distanze 8x8 di tofd e la posa sulla
mappa di quack-navd. Inglese: [README.md](README.md).

```sh
python3 scripts/map3d/record.py $STATE rec/ 300 [--sim-port 7872]   # accanto a una papera che esplora
python3 scripts/map3d/build.py rec/ out/ --voxel 0.02               # iso_*.png, top.png, cloud.ply, house3d.bin
python3 scripts/map3d/accuracy.py out/ rec/ <casa>.truth.json       # solo il gemello
```

- `record.py` tiene un frame ogni mezzo secondo mentre la papera è ferma.
- `build.py`: Depth Anything V2 **Small** (Apache-2.0; Base e Large sono
  non commerciali, non usati) dà a ogni frame una profondità relativa; i
  ritorni del sensore di profondità visti dalla camera la portano in
  metri, frame per frame; i frame si fondono in un TSDF sul sistema della
  mappa con i colori della camera, e si disegnano in isometrica. Servono
  torch, transformers, Pillow.
- `house3d.bin` è la casa per la scheda **3D** di quack-control (il suo
  servizio `house3d`, `/var/lib/quack-control/house3d.bin` sulla papera):
  ~0,5 MB per la prima prova, con il nome della mappa, così la pagina ci
  disegna la papera dal vivo solo su quella mappa. Si copia come dice il
  README di quack-control.
- Prima prova (2026-10-14, casa_arredata, velstand, 5 minuti di
  esplorazione, 42 % della casa mappato): 248 frame fusi su 296; contro la
  verità del gemello, il pavimento entro 1 cm (mediana), pareti e mobili
  2,3 cm di mediana, il 96 % entro 10 cm.
- Solo nel simulatore la camera della testa guarda all'indietro: il MJCF
  di microduck_rl mette `head_camera` a 180 gradi dal sito `tof` (la
  papera è a posto: `frames.camera` di robotd guarda avanti). Pollen lo
  sa e la gira nel proprio body_server (`sim/camera.py`, 2026-09-02),
  lasciando il MJCF com'è; il vecchio body_server del gemello no, quindi
  `viewer/body_with_map.py` fa lo stesso mezzo giro attorno all'x della
  camera, solo se la camera guarda ancora lontano dal sensore
  (`TWIN_CAMERA_BACKWARD=1` tiene quella del modello).
- Sulla papera: gli assi della camera rispetto a `frames.camera` e gli
  intrinseci (`media.video`) vanno verificati con la stessa
  sovrapposizione dei ritorni del sensore sull'immagine.
