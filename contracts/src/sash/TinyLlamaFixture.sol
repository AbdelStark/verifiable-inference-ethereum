// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.24;

import {QM31} from "../field/QM31.sol";
import {SashTypes} from "./SashTypes.sol";

/// @title TinyLlamaFixture
/// @notice Primary-source 23-segment TinyLlama-1.1B-Chat-v1.0 SASH proof constants (`y-attnout-w12`).
library TinyLlamaFixture {
    bytes32 internal constant MODEL_ID = 0x3def3a0832d3d82a82283200010c182132ac7d9eec681b772a95834f29e43c2a;
    bytes32 internal constant MANIFEST_ROOTS_HASH = 0xfd8ecd8c79795b3a79e426d60a912d0a8a160188539c9b12dd3803b249c55b26;
    bytes32 internal constant PUBLIC_DIGEST = 0x3be033a69fa7b682dda5bc0a3fb507a886e2ebd0fb77ae26e6c16408ecc4be10;
    uint16 internal constant NUM_SEGMENTS = 23;
    uint16 internal constant SEQ_LEN = 32;
    uint32 internal constant EOS_TOKEN_ID = 2;
    uint16 internal constant B0_REL_DRAWS = 29;
    uint16 internal constant HEAD_REL_DRAWS = 24;

    function promptIds() internal pure returns (uint32[] memory out) {
        out = new uint32[](25);
        out[0] = 529;
        out[1] = 29989;
        out[2] = 1792;
        out[3] = 29989;
        out[4] = 29958;
        out[5] = 13;
        out[6] = 1170;
        out[7] = 278;
        out[8] = 7483;
        out[9] = 310;
        out[10] = 3444;
        out[11] = 297;
        out[12] = 697;
        out[13] = 3273;
        out[14] = 10541;
        out[15] = 29889;
        out[16] = 2;
        out[17] = 13;
        out[18] = 29966;
        out[19] = 29989;
        out[20] = 465;
        out[21] = 22137;
        out[22] = 29989;
        out[23] = 29958;
        out[24] = 13;
    }

    function answerIds() internal pure returns (uint32[] memory out) {
        out = new uint32[](8);
        out[0] = 1576;
        out[1] = 7483;
        out[2] = 310;
        out[3] = 3444;
        out[4] = 338;
        out[5] = 3681;
        out[6] = 29889;
        out[7] = 2;
    }

    function tokIn() internal pure returns (uint32[] memory out) {
        out = new uint32[](32);
        out[0] = 529;
        out[1] = 29989;
        out[2] = 1792;
        out[3] = 29989;
        out[4] = 29958;
        out[5] = 13;
        out[6] = 1170;
        out[7] = 278;
        out[8] = 7483;
        out[9] = 310;
        out[10] = 3444;
        out[11] = 297;
        out[12] = 697;
        out[13] = 3273;
        out[14] = 10541;
        out[15] = 29889;
        out[16] = 2;
        out[17] = 13;
        out[18] = 29966;
        out[19] = 29989;
        out[20] = 465;
        out[21] = 22137;
        out[22] = 29989;
        out[23] = 29958;
        out[24] = 13;
        out[25] = 1576;
        out[26] = 7483;
        out[27] = 310;
        out[28] = 3444;
        out[29] = 338;
        out[30] = 3681;
        out[31] = 29889;
    }

    function tokOut() internal pure returns (uint32[] memory out) {
        out = new uint32[](32);
        out[0] = 12739;
        out[1] = 5205;
        out[2] = 29989;
        out[3] = 29958;
        out[4] = 13;
        out[5] = 4391;
        out[6] = 263;
        out[7] = 2211;
        out[8] = 4272;
        out[9] = 278;
        out[10] = 322;
        out[11] = 278;
        out[12] = 10541;
        out[13] = 10541;
        out[14] = 29889;
        out[15] = 13;
        out[16] = 29871;
        out[17] = 29958;
        out[18] = 29989;
        out[19] = 465;
        out[20] = 22137;
        out[21] = 29989;
        out[22] = 29958;
        out[23] = 13;
        out[24] = 1576;
        out[25] = 7483;
        out[26] = 310;
        out[27] = 3444;
        out[28] = 338;
        out[29] = 3681;
        out[30] = 29889;
        out[31] = 2;
    }

    function segments() internal pure returns (SashTypes.SegmentClaim[] memory segs) {
        segs = new SashTypes.SegmentClaim[](23);
        segs[0] = SashTypes.SegmentClaim({segmentIndex: 0, relationDraws: 29, preRoot: 0x7da3534f535fe416c9d9aa0e8f765e3da5ac8c772d0ef35e6f303103c099466f, modelRoot: 0xa45136572e13045f3415eb12e82c437a0a83f276fb2699765b32f70c00c3de34, traceRoot: 0xd937eb517f80d61ed39333010dd9f12fc64a9a07d2706675d75dda2715dc6e57, residualInRoot: 0x91d1a7303ec5ab248298bd5a6581c229759fa25b6058a10ae13cf6302bb23c46, residualOutRoot: 0x2205996986dfe255cf1f5b5e53ad4a439ef2e45ad712e15c0ce09d495acd3d57, tokenHornerClaim: QM31.Elem(1205748927, 962712399, 1890095260, 1449788258)});
        segs[1] = SashTypes.SegmentClaim({segmentIndex: 1, relationDraws: 0, preRoot: 0x5be40f222f8d2732d415252791bb2230ce8e506cd0913d392ed26f42e74c727e, modelRoot: 0x51a9264b1e87696045cfc754df73780eb58d3027f4e8cf7526cf7d4635d7c112, traceRoot: 0x2482547c4a26a10a51ea1c29204f9b3fcf646868530b325f40d7943fc6399903, residualInRoot: 0x2205996986dfe255cf1f5b5e53ad4a439ef2e45ad712e15c0ce09d495acd3d57, residualOutRoot: 0xf7996a5c81ad29219ea3b1088356d17fdc14ae4cca97e12ac4926206a3ff4173, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[2] = SashTypes.SegmentClaim({segmentIndex: 2, relationDraws: 0, preRoot: 0x077e5d63e3d04e472068a60a036d4a382a956f0f7f08820c73b07207ad54c52b, modelRoot: 0x5dc85d1ed891fb1063de9945e830b6281f996867713a166d684c4c650993ca70, traceRoot: 0x55b5bb685b650174e7a08d0d1eb6f34d4deb227026caca01ff6c98414db32b29, residualInRoot: 0xf7996a5c81ad29219ea3b1088356d17fdc14ae4cca97e12ac4926206a3ff4173, residualOutRoot: 0xd2a08b54178749148012f917a59d8b301ab4153a960605320e77a547849b5049, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[3] = SashTypes.SegmentClaim({segmentIndex: 3, relationDraws: 0, preRoot: 0x09d0d9627a332933303d0537aafa320f6608e82748d409020bfb781479474c37, modelRoot: 0xf8154b1d7bab8d6dc3625c6b1f379c1559b232218e3a983aa77a550a65277d01, traceRoot: 0x462d103da400ec42578d544d7e3af943f5e17d2ffb388c40d5fa9e32a3ee1513, residualInRoot: 0xd2a08b54178749148012f917a59d8b301ab4153a960605320e77a547849b5049, residualOutRoot: 0x05876521ef07fe286afd1c5e0d2702673e2cec6a16c2306a35560f5dd58f107d, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[4] = SashTypes.SegmentClaim({segmentIndex: 4, relationDraws: 0, preRoot: 0xbd1fc61f5f982a2e73d17d72e99f9c131b77ac2bcf52184f980065274cc85274, modelRoot: 0x433b4e05076a621c562d5a2a39e2d476456cf4626f4dcb7683d941747ff2fa06, traceRoot: 0x8683970cc5c8e1729ce073652fa23b5e265fd530053cc91b5265dd7f6068f45b, residualInRoot: 0x05876521ef07fe286afd1c5e0d2702673e2cec6a16c2306a35560f5dd58f107d, residualOutRoot: 0x8dd3c33d34360a2daf65153c291f4044f8fdb74d67e42a01dab3f779229e1554, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[5] = SashTypes.SegmentClaim({segmentIndex: 5, relationDraws: 0, preRoot: 0x2064a826aa1fad1dc204651340e1545512bb0f40745e053b1122fd3ef0e53517, modelRoot: 0x0373d747a042004fa42b0f38e9aba2495f75f16406575407c6390d3b80218926, traceRoot: 0xcfb0930ac9b7130f3830f96750eac024e50fbf1c2a6e43687084c56578bb6962, residualInRoot: 0x8dd3c33d34360a2daf65153c291f4044f8fdb74d67e42a01dab3f779229e1554, residualOutRoot: 0x9bf4164186b8b655af562f7a049cfb0d5f0c0a422378945f6e60227843783119, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[6] = SashTypes.SegmentClaim({segmentIndex: 6, relationDraws: 0, preRoot: 0x20dcc71db910e3266f154f6b67b191785f3aea1e37e33228d434404ad6844301, modelRoot: 0x90e0c66ac9749860d82b085bd709fb741f9c3c007c15b47f36d8673ba9bc3362, traceRoot: 0xdc6a426f94a1ce5399f23e3cee336145bb03797fedc1cb45d1d72062a245ca16, residualInRoot: 0x9bf4164186b8b655af562f7a049cfb0d5f0c0a422378945f6e60227843783119, residualOutRoot: 0x1f5d5d487e92af63eae4884e80822727ad6de40f51037c4046f5c22969898813, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[7] = SashTypes.SegmentClaim({segmentIndex: 7, relationDraws: 0, preRoot: 0x9055d94ba3b3884c8e79c935d26d960511a6666e57d34119a64a5c48efb05a0e, modelRoot: 0x69d21e224e749a2ae6916034d981922c890904076302ee7d17899438312a0504, traceRoot: 0xdc82630c7ebbdc4374cef000bef6394ddc470f715f7334475768e87b9a10bb10, residualInRoot: 0x1f5d5d487e92af63eae4884e80822727ad6de40f51037c4046f5c22969898813, residualOutRoot: 0xe835062f146adb7c7cc0e8107e80724b05252e4e2bc69220d1577137baf8a307, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[8] = SashTypes.SegmentClaim({segmentIndex: 8, relationDraws: 0, preRoot: 0xb529454de00f9104eecfe77997e69025d763c925ec021944dc15772013dd8f1b, modelRoot: 0x32d2615fec3a33171283676c7c53681d28c52752db3004270cc4e847f6983055, traceRoot: 0x8ae0271271fedf6c06e28c2bb7fd2d42a690ba498ba0641206da2064abebb11c, residualInRoot: 0xe835062f146adb7c7cc0e8107e80724b05252e4e2bc69220d1577137baf8a307, residualOutRoot: 0xfbec217047197929f165de6ed34b865af65a9c1baf069675c8629371d3c98849, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[9] = SashTypes.SegmentClaim({segmentIndex: 9, relationDraws: 0, preRoot: 0x286cab2c78867c5aff90ac1ddc979b360418c71dadd46225d3b5e92f22429334, modelRoot: 0xcdba4b3ccace4b48312bf64d87506169419d4e3ec9833f37b843e64fc1ff9e20, traceRoot: 0xaa37ac2a09a6be1a3e7ea361f73b1f026b806b424f03f46aa84ba33fb836104b, residualInRoot: 0xfbec217047197929f165de6ed34b865af65a9c1baf069675c8629371d3c98849, residualOutRoot: 0xad4e5d73a96be83b2e501920c42a5578eec8dc3b5d8b6c460e14e9740fc9796e, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[10] = SashTypes.SegmentClaim({segmentIndex: 10, relationDraws: 0, preRoot: 0x1d8d4104f1a331433c64fe39096a083ad8adbf482b8dde726d94636a856bb009, modelRoot: 0x017e121ef98b930e8df4d511e3bdb472dbf03f1c747a6f3db1cbe40a59248431, traceRoot: 0x9e5fed73464d6f750023714682aa571eb1df90322f22ac4955572e60cfe4aa25, residualInRoot: 0xad4e5d73a96be83b2e501920c42a5578eec8dc3b5d8b6c460e14e9740fc9796e, residualOutRoot: 0xcdb3dd6be02bb80d98eb674056980655dd598c1fa19a1a26e72ce2387c5ddd7f, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[11] = SashTypes.SegmentClaim({segmentIndex: 11, relationDraws: 0, preRoot: 0xa498b7365e9da476b4df22287e39144a743eee0865d58e3effa2ac76219d095a, modelRoot: 0x097c62612879a51c2ec0087258477d352562173b0b02c64529ad5a2c259c6c77, traceRoot: 0xa5d55b3d25d4d75f341f022b93331122122c3246de799c78ce86c7535ea3f44c, residualInRoot: 0xcdb3dd6be02bb80d98eb674056980655dd598c1fa19a1a26e72ce2387c5ddd7f, residualOutRoot: 0x764fb3011523f1556ff6464e3bd89778aaed5f5acb3c266484aa622ec93e185d, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[12] = SashTypes.SegmentClaim({segmentIndex: 12, relationDraws: 0, preRoot: 0x94e17779c22b263094fa6b2e90ebb11572014c4e56eb175fce23b962d813ea26, modelRoot: 0x1ace801a02679610ebe5c512d70cee16fab9ec6d234a081e649877289b3f8c7c, traceRoot: 0x6453cd7539688240b1c8747b6395eb23bd76683db60266675ea9f023c2e91624, residualInRoot: 0x764fb3011523f1556ff6464e3bd89778aaed5f5acb3c266484aa622ec93e185d, residualOutRoot: 0x43fa1b4f884f277ab001372006e2d05922637b4406e81d4f0d0a03038466d130, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[13] = SashTypes.SegmentClaim({segmentIndex: 13, relationDraws: 0, preRoot: 0x0764d647d02b6d1691aa1b2fc694c935b7745214b788e32ac9ca8835b93e4c2d, modelRoot: 0xfac0a223d7e5787dcb9d05007108d26797836e7d5d7d15609f47815883312a49, traceRoot: 0x36df796aebf0b50208843f296997ef4c15f98b2fdc851e1ca0a8e23290e65a2f, residualInRoot: 0x43fa1b4f884f277ab001372006e2d05922637b4406e81d4f0d0a03038466d130, residualOutRoot: 0xffcf72112f740c45b9240c29353ebc7ae9f164427cdef82b7f51746edc671918, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[14] = SashTypes.SegmentClaim({segmentIndex: 14, relationDraws: 0, preRoot: 0xfa811661bb74d42ceac9521267705045c18e027defa3db6b0ed73b7a07e05a3c, modelRoot: 0x1dcd8411b339156a9807277313e2b55279928e0b61b9d571e2c807229238a04f, traceRoot: 0xd25ef77926621b53aba71d39a74ca01dcf646b1059eaa603bc6a615329d9657b, residualInRoot: 0xffcf72112f740c45b9240c29353ebc7ae9f164427cdef82b7f51746edc671918, residualOutRoot: 0xc495b67d744a1411b6c15e576baac96b5af1551275acd53641e9197ce6c1716b, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[15] = SashTypes.SegmentClaim({segmentIndex: 15, relationDraws: 0, preRoot: 0x63195358dcf82d30d834b7460ac5227822bb9b09e99e9f4965d414047945df73, modelRoot: 0x2003e608127d4463bc9540352efe876c27a2067f1c38756236e4e8366eb8dc51, traceRoot: 0x9e464e3fabc7cc2c659a5c399c534e542127257d614de97b1a8f2d5263b9324c, residualInRoot: 0xc495b67d744a1411b6c15e576baac96b5af1551275acd53641e9197ce6c1716b, residualOutRoot: 0x56a6804257a1883b59dce61beb01561bb633536cd1308013e442e03192fad52c, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[16] = SashTypes.SegmentClaim({segmentIndex: 16, relationDraws: 0, preRoot: 0x7630d431e0bf797993f5a171d80cc70dc4591446137d48180f5f4164f06ea268, modelRoot: 0x88819c0d6e9727231a93704cf4954e1286a95226e24f162fed218a0c5989f23a, traceRoot: 0x2eb855493af4151332e95f2bae7f430a963a0576226e454ec689ce4164bbf21d, residualInRoot: 0x56a6804257a1883b59dce61beb01561bb633536cd1308013e442e03192fad52c, residualOutRoot: 0xeca1dd7e2c52df35f190d928f2b7b8186fb3681a69feec12b510f822f5e99774, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[17] = SashTypes.SegmentClaim({segmentIndex: 17, relationDraws: 0, preRoot: 0x06c8a320b1ded47d68582b73a6b26657d10956586633866a27b6ff1ceb526b54, modelRoot: 0xae540044d4576c7460ac6976c5fd6868ccf00f0f9d1f043bd927d371dc2c7650, traceRoot: 0xfb2ac4080a86c46f765086701cbb667745d4495a4c017d3bb1ffa5310ec4ec0a, residualInRoot: 0xeca1dd7e2c52df35f190d928f2b7b8186fb3681a69feec12b510f822f5e99774, residualOutRoot: 0x4540ee334611a77072d0501834d34261d7722451e069d00ebff467099de3dd2d, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[18] = SashTypes.SegmentClaim({segmentIndex: 18, relationDraws: 0, preRoot: 0xe29ee538b0353b2ed4599512b5529464d312445b049a3135ec70c925bab2be02, modelRoot: 0xc9dafc665a1aa75d353b0d1ab2478f51582cb517d38412765b5779455342af56, traceRoot: 0x8fbece577db95c0ed4b8530d9d793f7f17a4f71356365b2a7d63690812b1e068, residualInRoot: 0x4540ee334611a77072d0501834d34261d7722451e069d00ebff467099de3dd2d, residualOutRoot: 0xeefab97547debc178419596a4a83a97f12ba1d64258f2c7b23c8094ab328e211, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[19] = SashTypes.SegmentClaim({segmentIndex: 19, relationDraws: 0, preRoot: 0xc1fbe864087ead6284c25a6b2767e941ed528b6339bf357922887f011df4f821, modelRoot: 0x4b3e771b48de502d93502c2e6971a454535b6f1203260d5de78f2a6bb0576743, traceRoot: 0x081691395a4b703923d16354d819f82fb3017c3e31c73f7b53ac244aa20bf940, residualInRoot: 0xeefab97547debc178419596a4a83a97f12ba1d64258f2c7b23c8094ab328e211, residualOutRoot: 0x7867bb7700da9b12d72d362702bdd35ba7f4dc39ff42b737e7df2e1a5c507b19, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[20] = SashTypes.SegmentClaim({segmentIndex: 20, relationDraws: 0, preRoot: 0xba8b805846d9aa0bba01e936a7ef114aea99e7303a9a1b76f71b287d4fe39357, modelRoot: 0x1bb49a1a9dbf6a4ec11db3057c276319f186dd3f510db70e0ccc227cf2613258, traceRoot: 0xf47cb3131ac63468cc28d826e4a5552ba6e4a72792a25d2f355bc46f7931db66, residualInRoot: 0x7867bb7700da9b12d72d362702bdd35ba7f4dc39ff42b737e7df2e1a5c507b19, residualOutRoot: 0xaa13da41a635d22ee808c46983c7df0f2876e040f4d7b720e67ecc7ba037bb44, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[21] = SashTypes.SegmentClaim({segmentIndex: 21, relationDraws: 0, preRoot: 0x465cba2077dbf67cbd803b284bf9c82177d3425b15bf98730f73f97f7da3f20a, modelRoot: 0x32e9fe4814b0b03ef85ba94ac682ef41cea0744664db161c6bbac91857abc466, traceRoot: 0xd3d6805a886a3b74b49a490b63e20879df86944838253c721135e82a6a356469, residualInRoot: 0xaa13da41a635d22ee808c46983c7df0f2876e040f4d7b720e67ecc7ba037bb44, residualOutRoot: 0x9ce87154ce7ff652133b8455f646bb4afd6ff83cf839d85a7d9ce76551b29225, tokenHornerClaim: QM31.Elem(0, 0, 0, 0)});
        segs[22] = SashTypes.SegmentClaim({segmentIndex: 22, relationDraws: 24, preRoot: 0x3119373feaf0611ffaa7275ba4a6d031e33eeb3e2710ff41b1a9582431a8e73f, modelRoot: 0xe26a2e1be19e2b6d3f1ea17dc311e16c8fb1b523633e4e6a754720486220ce12, traceRoot: 0x8eeda8657770717a942bd218b852681bc76de5181169332bfa62586264b15a14, residualInRoot: 0x9ce87154ce7ff652133b8455f646bb4afd6ff83cf839d85a7d9ce76551b29225, residualOutRoot: 0x0000000000000000000000000000000000000000000000000000000000000000, tokenHornerClaim: QM31.Elem(754377417, 2109312062, 1514847, 2019687017)});
    }
}
