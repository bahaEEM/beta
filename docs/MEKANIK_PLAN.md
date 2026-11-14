# Mekanik plan — oyunu eksiksiz bitirmek (2026-10-12)

Kullanıcı kararı: fazlar kalktı. Hedef, oyunu bugünkü grafiklerle **mekanik olarak eksiksiz** bitirmek; Generals tarzı görsel
giydirme en sona. Oyunun özü: satranç / HoI4 gibi düşündüren, pusulu, radarlı, elektronik harpli, sert ve gerçekçi savaş.
Bu belge yapılacakların tek listesidir; her paket bitince işaretlenir, kurallar docs/design.md'ye K-kimliğiyle girer.

## Bitenler (özet)
Tur/WEGO/determinizm · görünürlük filtresi · hareket + yollar · optik/radar/ESM/üçgenleme/karıştırma · beklenen hasar, nişan,
ateş kuralları · haberleşme ağı (karargâh, telsiz röle, kablo) · topçu/füze · hava (İHA, SİHA, helikopter) + hava savunma ·
ZMA + piyade taşıma · bölge noktaları + puan + zafer · yakıt/mühimmat + ikmal kamyonu · istemci modelleri ve efektleri.

## Paketler (sırayla)

### M1 — Harita: bölgeler, deniz, arazi örtüsü
- Hasandağı haritasının doğu yarısı denize çevrilir (test amaçlı kıyı). Hücre tipi: kara / su / kıyı. ✔ (2026-11-14)
- Arazi örtüsü (OSM): orman, yerleşim, açık alan. Orman ve yerleşim gizlenme ve koruma verir (M4'te kullanılır). ✔ veri (2026-11-14)
- Adlı bölgeler: gerçek yerleşim adlarıyla 25–35 bölge (OSM yer adları, elle seçilmiş). Bölge sahipliği, cephe hattı,
  bölge değeri (puan), bölgeler arası yol bağlantıları. Eski "bölge noktaları" bölgelere taşınır. ✔ (2026-11-14)
- Kullanıcı kararları (2026-11-14): kıyı ve 30 bölge onaylandı · bölge, merkezindeki yerleşim tutulunca el değiştirir (K-060 aynen) ·
  başlangıç: kuzey Mavi, güney Kırmızı, orta tarafsız · **puan yok**: düşman üssünü alan kazanır, 100. turda çok bölgesi olan (K-061).
  Yani "bölge değeri (puan)" maddesi düştü; bölgelerin değeri M3 (ikmal) ve M7 (gelir) ile gelecek.

### M2 — Yapılar ve kurulabilir sistemler
- Sabit yapılar (hareket etmeyen birimler): radar kulesi (uzun menzil hava+kara), sahra hastanesi, lojistik üs, depo,
  hava alanı, liman, karargâh sığınağı, hava savunma mevzisi.
- Kurulabilir sistemler: radar çadırı / mobil radar — yürüyüş ↔ konuşlu geçişi zaman alır; sadece konuşluyken çalışır.
- Sağlık aracı: yakınındaki piyadeyi tur sonunda iyileştirir; yaralıyı sahra hastanesine taşıyınca tam iyileşme.

### M3 — Fiziksel ikmal
- Kaynaklar: lojistik üs (tur başı üretim), depo, hava alanı, liman stoklar tutar.
- Taşıyıcılar: ikmal kamyonu (yolda), kargo uçağı (hava alanları arası), ikmal gemisi (limanlar arası). Yükle / boşalt emirleri.
- Bölge ikmal durumu: kendi bölgelerinden geçen yollarla bir kaynağa bağlı bölge "ikmalli" — içindeki birimler tur sonunda
  azar azar ikmal alır; kopuk bölgede ikmal yok, uzun süre kopuk kalan birim zayıflar.
- İkmal hattı kesme: düşman bölgeyi alırsa ya da kamyonu vurursa hat kopar.

### M4 — Gizlenme, pusu, baskı
- Gizlenme: ormanda/yerleşimde ve dururken görülme menzili düşer; hareket ve ateş görünürlüğü artırır (atış izi bir süre ifşa eder).
- Pusu: gizli + "karşılık"/"pusu" ateş kuralı; yakın menzilde ilk atış bonusu.
- Baskı (suppression): ateş altındaki birimin isabeti/hızı düşer, piyade siper alır; zarsız, birikimli bir değer.
- Aldatma: şişme/sahte birimler (tank, radar) — iz 1–2'de gerçek gibi görünür.

### M5 — Elektronik harp derinliği
- Telsiz yayını: emir alan/röle yapan birimler kısa süre yayın yapar → ESM görür. Telsiz sessizliği (EMCON) ateş kuralı gibi seçilir.
- Radar karşıtı füze: yayın yapan radara kilitlenir (ESM üçgenleme yeter).
- Karşı-batarya radarı: düşen/atılan mermiden atıcının konumunu bulur.
- İHA veri bağı karıştırma: karıştırıcı altındaki İHA/SİHA emir alamaz, eve döner.

### M6 — Deniz
- Gemi sınıfları: devriye botu, korvet/firkateyn (hava savunma radarı, gemisavar füze), denizaltı, çıkarma gemisi, ikmal gemisi;
  kıyı gemisavar füze bataryası.
- Sonar: aktif (yayın yapar, denizaltıyı bulur, duyulur), pasif (gürültülü gemilere kerteriz). Denizaltı sadece sonarla bulunur.
- Çıkarma: çıkarma gemisi kara birimi taşır, kıyıya indirir.

### M7 — Ekonomi ve takviye (HoI4)
- Bölgeler kaynak üretir; tur sonu gelir. Lojistik üste birim satın alma / takviye kuyruğu (birkaç tur sürer).
- Hasarlı birimlerin onarımı (depo/liman yanında), kayıpların ikmali.

### Her paketten sonra
Testler (K-kimlikli) + denge raporu + gerektiğinde istemci gösterimi (alt ajan) + commit + PROGRESS.md.
