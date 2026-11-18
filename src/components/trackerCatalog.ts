export type Category = 'base' | 'dlc' | 'fragments' | 'ashes' | 'preparation' | 'custom'
interface EntryText {
  name: string
  place: string
  reward?: string
}

export interface Entry extends EntryText {
  id: string
  quantity: number
  english?: EntryText
}

export function entryText(entry: Entry, lang: 'ko' | 'en'): EntryText {
  return lang === 'en' && entry.english ? entry.english : entry
}

export const categories: { id: Category; name: string; english: string }[] = [
  { id: 'base', name: '본편 추억 보스', english: 'Base Game Remembrance Bosses' },
  { id: 'dlc', name: 'DLC 추억 보스', english: 'DLC Remembrance Bosses' },
  { id: 'fragments', name: '그림자 나무 파편', english: 'Scadutree Fragments' },
  { id: 'ashes', name: '영혼 재', english: 'Revered Spirit Ashes' },
  { id: 'preparation', name: '다음 회차 준비', english: 'New Game Plus Checklist' },
  { id: 'custom', name: '나의 목표', english: 'My Goals' },
]

export function categoryLabel(id: Category, lang: 'ko' | 'en'): string {
  const category = categories.find(item => item.id === id)!
  return lang === 'en' ? category.english : category.name
}

const places = {
  '스톰빌 성': 'Stormveil Castle',
  '마술학원 레아 루카리아': 'Academy of Raya Lucaria',
  '케일리드 · 통곡 사구': 'Caelid · Wailing Dunes',
  '영원한 도읍 노크론 · 각해의 영지': 'Nokron, Eternal City · Hallowhorn Grounds',
  '부패한 호수 · 대회랑 너머': 'Lake of Rot · Beyond the Grand Cloister',
  '깊은 뿌리 밑바닥': 'Deeproot Depths',
  '겔미어 화산 · 화산관': 'Mt. Gelmir · Volcano Manor',
  '도읍 로데일': 'Leyndell, Royal Capital',
  '거인들의 산령': 'Mountaintops of the Giants',
  '모그윈 왕조묘': 'Mohgwyn Dynasty Mausoleum',
  '성수 버팀목 에브레펠': 'Elphael, Brace of the Haligtree',
  '무너지는 파름 아즈라': 'Crumbling Farum Azula',
  '재의 도읍 로데일': 'Leyndell, Ashen Capital',
  '황금 나무 내부': 'Inside the Erdtree',
  '탑의 도시 벨라트': 'Belurat, Tower Settlement',
  '엔시스 성채': 'Castle Ensis',
  '석관의 큰 구멍': 'Stone Coffin Fissure',
  '그림자의 성 · 암실': 'Shadow Keep · Dark Chamber',
  '그림자의 성 뒷문': 'Shadow Keep, Back Gate',
  '그림자 나무 기슭': 'Scadutree Base',
  '마누스 메테르 대교회 · 손가락의 산실': 'Cathedral of Manus Metyr · Finger Birthing Grounds',
  '미드라의 저택': 'Midra\'s Manse',
  '라우프의 옛 유적': 'Ancient Ruins of Rauh',
  '에니르 일림': 'Enir-Ilim',
  '묘지 평원': 'Gravesite Plain',
  '톱니산': 'Jagged Peak',
  '푸른 해안': 'Cerulean Coast',
  '카로의 숨겨진 묘지': 'Charo\'s Hidden Grave',
  '그림자 알터': 'Scadu Altus',
  '그림자의 성': 'Shadow Keep',
  '그림자의 성 교구': 'Shadow Keep, Church District',
  '은자의 강': 'Recluses\' River',
  '은자의 강 하류': 'Recluses\' River Downstream',
  '나락의 숲': 'Abyssal Woods',
  '라우프의 기슭': 'Rauh Base',
}
type Place = keyof typeof places

// Boss and remembrance names follow the Korean game localization. Each encounter
// has one stable key across both phases and across journeys.
type Boss = [id: string, name: string, englishName: string, place: Place, reward: string, englishReward: string]
const base: Boss[] = [
  ['godrick', '접목의 고드릭', 'Godrick the Grafted', '스톰빌 성', '접목의 추억', 'Remembrance of the Grafted'],
  ['rennala', '만월의 여왕 레날라', 'Rennala, Queen of the Full Moon', '마술학원 레아 루카리아', '만월의 여왕의 추억', 'Remembrance of the Full Moon Queen'],
  ['radahn', '별 부수는 라단', 'Starscourge Radahn', '케일리드 · 통곡 사구', '별 부수기의 추억', 'Remembrance of the Starscourge'],
  ['ancestor', '선조령의 왕', 'Regal Ancestor Spirit', '영원한 도읍 노크론 · 각해의 영지', '선조령의 왕의 추억', 'Remembrance of the Regal Ancestor'],
  ['astel', '암흑의 부산물 아스테르', 'Astel, Naturalborn of the Void', '부패한 호수 · 대회랑 너머', '암흑의 부산물의 추억', 'Remembrance of the Naturalborn'],
  ['fortissax', '사룡 포르삭스', 'Lichdragon Fortissax', '깊은 뿌리 밑바닥', '사룡의 추억', 'Remembrance of the Lichdragon'],
  ['rykard', '모독의 군주 라이커드', 'Rykard, Lord of Blasphemy', '겔미어 화산 · 화산관', '모독의 군주의 추억', 'Remembrance of the Blasphemous'],
  ['morgott', '흉조의 왕 모르고트', 'Morgott, the Omen King', '도읍 로데일', '흉조의 왕의 추억', 'Remembrance of the Omen King'],
  ['giant', '불의 거인', 'Fire Giant', '거인들의 산령', '불의 거인의 추억', 'Remembrance of the Fire Giant'],
  ['mohg', '피의 군주 모그', 'Mohg, Lord of Blood', '모그윈 왕조묘', '피의 군주의 추억', 'Remembrance of the Blood Lord'],
  ['malenia', '미켈라의 칼날 말레니아', 'Malenia, Blade of Miquella', '성수 버팀목 에브레펠', '부패의 여신의 추억', 'Remembrance of the Rot Goddess'],
  ['placidusax', '용왕 플라키두삭스', 'Dragonlord Placidusax', '무너지는 파름 아즈라', '용왕의 추억', 'Remembrance of the Dragonlord'],
  ['maliketh', '흑검 말리케스', 'Maliketh, the Black Blade', '무너지는 파름 아즈라', '흑검의 추억', 'Remembrance of the Black Blade'],
  ['godfrey', '첫 왕 고드프리 · 전사 호라 루', 'Godfrey, First Elden Lord · Hoarah Loux, Warrior', '재의 도읍 로데일', '호라 루의 추억', 'Remembrance of Hoarah Loux'],
  ['elden-beast', '황금률 라다곤 · 엘데의 짐승', 'Radagon of the Golden Order · Elden Beast', '황금 나무 내부', '엘데의 추억', 'Elden Remembrance'],
]

const dlc: Boss[] = [
  ['lion', '신수 사자무', 'Divine Beast Dancing Lion', '탑의 도시 벨라트', '사자무의 추억', 'Remembrance of the Dancing Lion'],
  ['rellana', '쌍월의 기사 렐라나', 'Rellana, Twin Moon Knight', '엔시스 성채', '쌍월의 기사의 추억', 'Remembrance of the Twin Moon Knight'],
  ['putrescent', '수렁의 기사', 'Putrescent Knight', '석관의 큰 구멍', '수렁의 추억', 'Remembrance of Putrescence'],
  ['messmer', '가시공 메스메르', 'Messmer the Impaler', '그림자의 성 · 암실', '가시공의 추억', 'Remembrance of the Impaler'],
  ['gaius', '노장 가이우스', 'Commander Gaius', '그림자의 성 뒷문', '멧돼지 탄 자의 추억', 'Remembrance of the Wild Boar Rider'],
  ['avatar', '그림자 나무의 화신', 'Scadutree Avatar', '그림자 나무 기슭', '영륜초의 추억', 'Remembrance of the Shadow Sunflower'],
  ['metyr', '손가락의 어머니 메티르', 'Metyr, Mother of Fingers', '마누스 메테르 대교회 · 손가락의 산실', '손가락의 어머니의 추억', 'Remembrance of the Mother of Fingers'],
  ['midra', '미친 불의 왕 미드라', 'Midra, Lord of Frenzied Flame', '미드라의 저택', '미친 불의 왕의 추억', 'Remembrance of the Lord of Frenzied Flame'],
  ['romina', '꽃봉오리의 성녀 로미나', 'Romina, Saint of the Bud', '라우프의 옛 유적', '꽃봉오리의 성녀의 추억', 'Remembrance of the Saint of the Bud'],
  ['consort', '약속의 왕 라단', 'Promised Consort Radahn', '에니르 일림', '신과 왕의 추억', 'Remembrance of a God and a Lord'],
]

// Pickup quantities cross-checked against the individual markers in
// https://github.com/dsameendra/tarnished-ledger/blob/main/js/data/counter-markers.js
// Locations checked against the gallery author's in-game photographs in
// fromsoftware posts 4757098, 4757155 and 4757256.
type Pickup = [id: string, place: Place, name: string, englishName: string, quantity?: number]
const fragments: Pickup[] = [
  ['consolation', '묘지 평원', '위로의 교회 제단', 'Altar in the Church of Consolation', 2],
  ['three-path', '묘지 평원', '세갈래 길의 십자', 'Three-Path Cross'],
  ['main-gate', '묘지 평원', '벨라트 정문 앞 십자', 'Main Gate Cross outside Belurat'],
  ['scorched', '묘지 평원', '불에 탄 폐허 동쪽 묘지의 항아리 그림자', 'Pot-carrying shadow in the graveyard east of Scorched Ruins'],
  ['prospect', '묘지 평원', '조망도시 서쪽 묘지의 항아리 그림자', 'Pot-carrying shadow in the graveyard west of Prospect Town'],
  ['ensis-camp', '묘지 평원', '엔시스 성채 앞 병영의 석상', 'Statue in the camp outside Castle Ensis'],
  ['pillar', '묘지 평원', '기둥언덕의 십자', 'Pillar Path Cross'],
  ['belurat', '탑의 도시 벨라트', '무너진 벽 안쪽의 십자', 'Cross behind the collapsed wall'],
  ['ensis', '엔시스 성채', '성채 관문의 십자', 'Cross at Castle Ensis Checkpoint'],
  ['boulders', '톱니산', '낙석이 굴러오는 길 끝', 'End of the path with falling boulders'],
  ['coast-cave', '푸른 해안', '아인 우두머리가 지키는 해안 동굴', 'Coastal cave guarded by a Demi-Human Chief'],
  ['coast-cross', '푸른 해안', '푸른 해안의 십자', 'Cerulean Coast Cross'],
  ['charo', '카로의 숨겨진 묘지', '붉은 꽃밭의 하마', 'Hippopotamus in the red flower field'],
  ['fissure', '석관의 큰 구멍', '큰 구멍의 십자', 'Fissure Cross'],
  ['highroad', '그림자 알터', '가도의 십자', 'Highroad Cross'],
  ['moorth-cross', '그림자 알터', '모스의 폐허의 십자', 'Moorth Ruins Cross'],
  ['moorth-camp', '그림자 알터', '모스의 폐허 서쪽 병영의 석상', 'Statue in the camp west of Moorth Ruins'],
  ['moorth-pot', '그림자 알터', '모스의 폐허의 항아리 그림자', 'Pot-carrying shadow in Moorth Ruins'],
  ['moorth-lake', '그림자 알터', '모스의 폐허 북쪽 연못의 석상', 'Statue by the pond north of Moorth Ruins'],
  ['scaduview', '그림자 알터', '그림자 나무를 향한 십자', 'Scaduview Cross'],
  ['crusade', '그림자 알터', '성전 교회 안 제단', 'Altar in the Church of the Crusade', 2],
  ['keep-camp', '그림자 알터', '그림자의 성 정문 앞 병영의 석상', 'Statue in the camp outside Shadow Keep\'s main gate'],
  ['golden-hippo', '그림자의 성', '정문의 황금 하마', 'Golden Hippopotamus at the main gate', 2],
  ['storehouse', '그림자의 성', '씨앗 보관고 4층 십자', 'Cross on the fourth floor of the Specimen Storehouse'],
  ['dark-chamber', '그림자의 성', '메스메르의 암실 앞 석상', 'Statue outside Messmer\'s Dark Chamber'],
  ['church-district', '그림자의 성 교구', '물에 잠긴 석상의 손', 'Hand of the submerged statue'],
  ['back-gate', '그림자의 성 뒷문', '축복 옆 기도실의 석상', 'Statue in the prayer room beside the site of grace'],
  ['chalice', '그림자의 성 뒷문', '가이우스 너머 그림자 나무의 성배', 'Scadutree Chalice beyond Commander Gaius', 5],
  ['coffin', '은자의 강', '폭포 아래 석관', 'Stone coffin below the waterfall'],
  ['hippo-west', '은자의 강 하류', '호수 서쪽 하마', 'Hippopotamus on the west side of the lake'],
  ['hippo-east', '은자의 강 하류', '호수 동쪽 하마', 'Hippopotamus on the east side of the lake'],
  ['abandoned', '나락의 숲', '폐교회 석상', 'Statue in the Abandoned Church', 2],
  ['woods', '나락의 숲', '나락의 숲 축복 북쪽 큰 나무', 'Large tree north of the Abyssal Woods site of grace'],
  ['temple', '라우프의 기슭', '신전도시 폐허의 높은 난간', 'High ledge in Temple Town Ruins'],
  ['rauh-hippo', '라우프의 옛 유적', '승강기로 내려간 연못의 하마', 'Hippopotamus in the pond below the lift'],
  ['rauh-pot', '라우프의 옛 유적', '전갈 구역 너머의 항아리 그림자', 'Pot-carrying shadow past the scorpions'],
  ['rauh-east', '라우프의 옛 유적', '동쪽 축복의 십자', 'Cross at the Rauh Ancient Ruins, East site of grace'],
  ['rauh-west', '라우프의 옛 유적', '서쪽 유적 안쪽 제단', 'Altar inside the western ruins'],
  ['spiral', '에니르 일림', '나선탑의 십자', 'Cross at Spiral Rise'],
  ['euporia', '에니르 일림', '나선탑에서 벨라트 방향으로 내려가는 제단', 'Altar on the descent from Spiral Rise toward Belurat'],
  ['frost', '에니르 일림', '냉기 신수 전사 옆 방', 'Room beside the frost-wielding Divine Beast Warrior'],
  ['antechamber', '에니르 일림', '정화의 방 전실 제단', 'Altar in the Cleansing Chamber Anteroom'],
]

const ashes: Pickup[] = [
  ['consolation', '묘지 평원', '위로의 교회 바깥 절벽 제단', 'Cliffside altar outside the Church of Consolation'],
  ['ailing', '묘지 평원', '병을 버리는 마을의 석상', 'Statue in the Ailing Village'],
  ['prospect', '묘지 평원', '조망도시 북동쪽 연못가', 'Pond northeast of Prospect Town'],
  ['ellac', '묘지 평원', '에라크 강 동굴의 석상', 'Statue in Ellac River Cave'],
  ['scorpions', '탑의 도시 벨라트', '입구 전갈 방의 제단', 'Altar in the scorpion room near the entrance', 2],
  ['bridge', '탑의 도시 벨라트', '뿔 전사 다리의 항아리 그림자', 'Pot-carrying shadow on the Horned Warrior\'s bridge'],
  ['tower', '탑의 도시 벨라트', '보스 구역으로 오르는 계단 앞 석상', 'Statue before the stairs leading to the boss arena'],
  ['theatre', '탑의 도시 벨라트', '신수 무대 위 승강기 뒤편', 'Behind the lift above the Theatre of the Divine Beast'],
  ['moorth', '그림자 알터', '모스의 폐허의 절벽 제단', 'Cliffside altar in Moorth Ruins'],
  ['bonny', '그림자 알터', '보니 마을 협곡 끝', 'End of the ravine in Bonny Village'],
  ['storehouse-one', '그림자의 성', '씨앗 보관고 1층 동물 표본', 'Animal specimen on the first floor of the Specimen Storehouse'],
  ['storehouse-seven', '그림자의 성', '씨앗 보관고 7층 매달린 표본', 'Hanging specimen on the seventh floor of the Specimen Storehouse'],
  ['unte', '은자의 강', '안테의 폐허 남쪽 제단', 'Altar south of the Ruins of Unte'],
  ['flies', '은자의 강', '파리의 마을 제단', 'Altar in the Village of Flies'],
  ['inquisitor', '미드라의 저택', '서고의 대형 촛불 심문관', 'Large candle-bearing inquisitor in the library'],
  ['chandelier', '미드라의 저택', '2층 작은 방 옆 샹들리에', 'Chandelier beside the small upstairs room'],
  ['temple', '라우프의 기슭', '신전도시 폐허 남쪽 절벽', 'Cliff south of Temple Town Ruins'],
  ['upper-pot', '라우프의 옛 유적', '유적 위층 입구의 항아리 그림자', 'Pot-carrying shadow at the upper entrance to the ruins'],
  ['warriors', '라우프의 옛 유적', '뿔 전사들이 지키는 석상', 'Statue guarded by Horned Warriors'],
  ['spring-pot', '라우프의 옛 유적', '서쪽 영기류 위의 항아리 그림자', 'Pot-carrying shadow above the western spiritspring'],
  ['first-rise', '에니르 일림', '제1탑 광장의 대형 심문관', 'Large inquisitor in the First Rise courtyard'],
  ['roots', '에니르 일림', '제1탑 광장 아래 나무뿌리 제단', 'Altar among the roots below the First Rise courtyard', 2],
  ['birds', '에니르 일림', '나선탑 너머 새가 내려오는 방', 'Room where birds swoop down beyond Spiral Rise'],
]

function bosses(list: Boss[]): Entry[] {
  return list.map(([id, name, englishName, place, reward, englishReward]) => ({
    id: 'boss-' + id, name, place, reward, quantity: 1,
    english: { name: englishName, place: places[place], reward: englishReward },
  }))
}
function pickups(prefix: string, list: Pickup[]): Entry[] {
  return list.map(([id, place, name, englishName, quantity = 1]) => ({
    id: prefix + id, name, place, quantity, english: { name: englishName, place: places[place] },
  }))
}

// Optional NG+ planning tasks informed by Elden Ring gallery post 71843 and
// https://eldenring.redmaw.dev/sheets/new-game-plus .
const preparation: [id: string, name: string, englishName: string, detail: string, englishDetail: string][] = [
  ['remembrance', '원하는 추억 보상 교환', 'Claim the Remembrance rewards you want',
    '복제할 추억을 정하고 영묘와 원탁의 엔야를 방문', 'Choose which Remembrances to duplicate, then visit a Wandering Mausoleum and speak to Enia at Roundtable Hold.'],
  ['npc', '진행 중인 NPC 보상 확인', 'Collect outstanding NPC rewards',
    '이번 회차에서 마무리할 의뢰를 나의 목표에 기록', 'Add the quests you want to finish this playthrough to My Goals.'],
  ['bell', '단석·은방울꽃 방울 전달', 'Offer your smithing stone and glovewort bell bearings',
    '원탁의 쌍둥이 노파에게 모은 방울을 건네고 상점 확인', 'Give your bell bearings to the Twin Maiden Husks at Roundtable Hold and check their stock.'],
  ['upgrade', '최종 강화 재료 확보', 'Collect final upgrade materials',
    '쓸 무기와 뼛가루를 정하고 고룡암의 단석과 큰 은방울꽃 수집', 'Choose which weapons and spirit ashes to upgrade, then collect Ancient Dragon Smithing Stones and the Great Gloveworts you need.'],
  ['flasks', '성배병 강화 확인', 'Check your Sacred Flask upgrades',
    '황금 종자와 성배의 물방울을 축복에서 사용', 'Use your Golden Seeds and Sacred Tears at a site of grace.'],
  ['spell', '마술과 기도 구입', 'Buy the sorceries and incantations you need',
    '얻어 둔 스크롤과 기도서로 배울 주문 확인', 'Check which spells your scrolls and prayerbooks have unlocked.'],
  ['build', '다음 빌드 준비', 'Prepare your next build',
    '장비와 탈리스만을 챙기고 물방울 유생 수량 확인', 'Gather your equipment and talismans and check how many Larval Tears you have.'],
  ['save', '회차 시작 전 세이브 백업', 'Back up your save before starting the next journey',
    '게임을 종료하고 세이브 관리에서 이번 회차 이름으로 보관', 'Close the game and create a backup in Save Manager using this journey\'s name.'],
]

export const catalog: Record<Exclude<Category, 'custom'>, Entry[]> = {
  base: bosses(base), dlc: bosses(dlc), fragments: pickups('fragment-', fragments), ashes: pickups('ash-', ashes),
  preparation: preparation.map(([id, name, englishName, place, englishDetail]) => ({
    id: 'prep-' + id, name, place, quantity: 1, english: { name: englishName, place: englishDetail },
  })),
}
