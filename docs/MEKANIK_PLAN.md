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
- ✔ (2026-11-14) K-120…K-123. Kullanıcı kararları: kurulum **otomatik** (durunca kurulur, hareket emrinde önce toplanır) ·
  kurulum isteyenler: topçu + füze rampası, kara gözetleme radarı, karıştırıcı + ESM, yeni mobil radar (hava savunma hariç) ·
  yeni piyade türleri: makineli tüfek timi (kurulur), roketatar timi (çok hafif kurulum) · yapılar **bölgeyle birlikte el değiştirir**.
- Oyunun felsefesi (kullanıcı, 2026-11-14): Go'nun alan hakimiyeti + günümüz teknolojisi + Sun Tzu — doğru zamanda çekilen, doğru yerden
  vuran, rakibini tahmin eden, ikmalini ve askerini iyi yöneten kazanır. Yeni mekanikler bu ölçüyle seçilir.

### M3 — Fiziksel ikmal
- Kaynaklar: lojistik üs (tur başı üretim), depo, hava alanı, liman stoklar tutar.
- Taşıyıcılar: ikmal kamyonu (yolda), kargo uçağı (hava alanları arası), ikmal gemisi (limanlar arası). Yükle / boşalt emirleri.
- Bölge ikmal durumu: kendi bölgelerinden geçen yollarla bir kaynağa bağlı bölge "ikmalli" — içindeki birimler tur sonunda
  azar azar ikmal alır; kopuk bölgede ikmal yok, uzun süre kopuk kalan birim zayıflar.
- İkmal hattı kesme: düşman bölgeyi alırsa ya da kamyonu vurursa hat kopar.
- ✔ (2026-11-14) K-130…K-134. Kullanıcı kararları: **bölge ağı + kamyon** (ağdaki birim kendiliğinden azar azar, ağ dışındaki kamyonla) ·
  kuşatılan birim **erir ve sonunda yok olur** (3 tur sonra tur başı %10) · yükleme/boşaltma **otomatik**, emir yok.
  İkmal gemisi deniz hareketi gerektirdiği için M6'ya bırakıldı. Hava birimlerinin yakıtı hâlâ modellenmiyor; her kendi bölgesinde mühimmat alabiliyorlar
  (hava alanı şartı M6/M7'de düşünülecek).

### M4 — Gizlenme, pusu, baskı
- Gizlenme: ormanda/yerleşimde ve dururken görülme menzili düşer; hareket ve ateş görünürlüğü artırır (atış izi bir süre ifşa eder).
- Pusu: gizli + "karşılık"/"pusu" ateş kuralı; yakın menzilde ilk atış bonusu.
- Baskı (suppression): ateş altındaki birimin isabeti/hızı düşer, piyade siper alır; zarsız, birikimli bir değer.
- Aldatma: şişme/sahte birimler (tank, radar) — iz 1–2'de gerçek gibi görünür.
- ✔ (2026-11-14) K-140…K-144. Kullanıcı kararları: pusu **yeni ateş kuralı** · ağır baskı altındaki piyade **yere yatar, ilerleyemez** ·
  sahte birimler **senaryoda hazır** (taraf başı 2 sahte tank + 1 sahte radar; satın alma M7'de).
  Plandan sapma: sahte birim "iz 1–2'de" değil, 800 m'den yakından görülene kadar gerçek görünür (yoksa optikle hemen anlaşılıyordu).

### M5 — Elektronik harp derinliği
- Telsiz yayını: emir alan/röle yapan birimler kısa süre yayın yapar → ESM görür. Telsiz sessizliği (EMCON) ateş kuralı gibi seçilir.
- Radar karşıtı füze: yayın yapan radara kilitlenir (ESM üçgenleme yeter).
- Karşı-batarya radarı: düşen/atılan mermiden atıcının konumunu bulur.
- İHA veri bağı karıştırma: karıştırıcı altındaki İHA/SİHA emir alamaz, eve döner.
- ✔ (2026-11-14) K-150…K-155. Kullanıcı kararları: sessizdeki birim **yeni emir alamaz** · radar avcısı **kamikaze İHA rampası** ·
  karşı-batarya radarı topu bulunca **topçu kendiliğinden karşılık verir**.

### M6 — Deniz
- Gemi sınıfları: devriye botu, korvet/firkateyn (hava savunma radarı, gemisavar füze), denizaltı, çıkarma gemisi, ikmal gemisi;
  kıyı gemisavar füze bataryası.
- Sonar: aktif (yayın yapar, denizaltıyı bulur, duyulur), pasif (gürültülü gemilere kerteriz). Denizaltı sadece sonarla bulunur.
- Çıkarma: çıkarma gemisi kara birimi taşır, kıyıya indirir.

### M7 — Ekonomi ve takviye (HoI4)
- Bölgeler kaynak üretir; tur sonu gelir. Lojistik üste birim satın alma / takviye kuyruğu (birkaç tur sürer).
- Hasarlı birimlerin onarımı (depo/liman yanında), kayıpların ikmali.
- ✔ (2026-11-15) K-170…K-173. Kullanıcı kararları: gelir **yalnız ikmalli bölgelerden** · birim **türüne göre yapıda** (kara: lojistik üs,
  hava: hava alanı, deniz: liman) ve **birkaç tur sonra** çıkar; yapı düşerse sipariş yanar · **onarım ve inşaat bu pakette yok**, M8'e alındı.
  Fiyatlar, süreler ve gelir tahmini; denge ayarı yapılmadı.

### M8 — İstihkâm: onarım ve inşaat (kullanıcı isteği, 2026-11-15; ayrıntısı birlikte düşünülecek)
- Kullanıcı: "istihkâm çok hayati". Onarımın tek mantıklı yolu bir **istihkâm birimi** (C&C'de birim yenilemenin bir şey gerektirmesi gibi).
- İnşaat da istihkâm birimiyle ve **ek ücretle**; uzun sürer (2–3 tur). Yalnız gerçekte 2–3 günde kurulabilecek şeyler:
  **onarım çadırı**, **iyileştirme çadırı** (yalnız piyade), **baz istasyonu kulesi**, **kazılmış sığınak** (obüse dayanıklı, kara birimlerine
  karşı iyi savunma), hava savunma mevzisi. Hava alanı gibi büyük yapılar **inşa edilemez**; beton tankeri gibi yeni araç istenmiyor.
- Kullanıcı kararları (2026-11-15):
  - **Onarım:** istihkâm, yanındaki (300 m) hasarlı **kara araçlarını ve yapıları** onarır; tur başı azami canın **%20**'si, **paralı**
    (birim fiyatıyla orantılı; para yoksa durur). Piyade sağlık aracında/hastanede kalır. Gemi ve hava birimi onarımı bu pakette yok.
  - **Sığınak = mevzi:** yanındaki piyade (en fazla 2 tim) siperde sayılır — topçu hasarı çok düşer, direkt ateşte iyi korunur, zor baskılanır,
    **ateş etmeye devam eder**. Araç giremez.
  - **Baz istasyonu kulesi:** sabit, uzun menzilli **telsiz rölesi** (K-081); bölgeyle el değiştirir, karıştırılabilir, vurulabilir.
  - **İnşaat:** istihkâm yapıyı durduğu yere, yalnız kendi bölgesinde kurar; para **peşin**; 2–3 tur **başında bekler**. Ayrılırsa inşaat
    **durur** (yanmaz), dönünce ya da başka istihkâm gelince sürer; istihkâm ölürse yarım yapı kalır. İnşaat sırasında onarım yapamaz; silahsızdır.
  - **Mayın tarlası (tanksavar):** giren **düşman aracı** hasar alır ve o tur durur; piyade ve dost birimler zarar görmez; tarla patladıkça tükenir.
    Gizlidir: düşman piyadesi / keşif aracı / istihkâmı çok yaklaşınca (≈200 m) ya da bir araç basınca görünür. **İstihkâm** üstünde 1 tur durarak temizler.
  - **Tanksavar hendeği:** istihkâm 2 turda kısa bir hat (≈300 m) kazar; **hiçbir kara aracı geçemez** (dost dahil), rota dolanır; piyade geçer.
    Gizli değildir. Herhangi bir istihkâm 1 turda doldurur.
  - Köprü yok (haritada nehir yok).
- ✔ M8a (2026-11-15) K-180…K-182: istihkâm aracı, onarım, inşaat, sığınak. Sayılar tahmini. M8b (mayın + hendek) bekliyor.
- Uygulama sırası: **M8a** istihkâm birimi + onarım + inşaat (çadırlar, kule, sığınak, hava savunma mevzisi) → **M8b** mayın + hendek.

### Her paketten sonra
Testler (K-kimlikli) + denge raporu + gerektiğinde istemci gösterimi (alt ajan) + commit + PROGRESS.md.
