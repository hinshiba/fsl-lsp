# 各種コマンド
FSLC	 = nfc
IVERILOG = iverilog
VVP	 = vvp

# FSL記述を増やしたら，下記の行に追加する
SRCS 	= mult32.fsl test_mult32.fsl
# テストベンチのターゲットモジュール
TARGET	= test_mult32
# シミュレーション結果を保存するファイル
TEMPFILES = test_mult32.result
# テストベクタ（入力信号パターン，結果比較用）
TESTVECTORS = test_mult32.pat test_mult32.result.sample

# make だけのときは，-verilog オプション等なしコンパイル
all:	compile

# シミュレーションするとき make sim
sim:	test_mult32.result

# .vvpファイルを生成するときは make vvp
vvp:	$(TARGET).vvp

# コンパイル（エラーチェックのみ）
compile: $(SRCS)
	$(FSLC) $^ 

# Verilog HDLファイルをつくるときは，make v あるいは make verilog
v:	verilog
verilog: $(SRCS:.fsl=.v)

# Verilog HDL ファイルの生成（コンパイル）
$(SRCS:.fsl=.v): $(SRCS)
	$(FSLC) $^ -verilog -target $(TARGET)

# .vvp ファイル（シミュレーションスクリプト）の生成
test_mult32.vvp: $(SRCS:.fsl=.v)
	$(IVERILOG) -o $@ $^

# .vvpファイルを実行（シミュレーション）して，結果をファイルに保存
test_mult32.result: $(TARGET).vvp
	$(VVP) $(TARGET).vvp | tee $@
	@echo "result file '$@' generated"

# シミュレーション結果と比較用パターンとの差分をとる
diff: test_mult32.result test_mult32.result.sample
	diff test_mult32.result test_mult32.result.sample || exit $$(($$? - 1))


clean:
	rm -f *~ $(TEMPFILES) 
	rm -f $(SRCS:.fsl=.v)
	rm -f *.vcd

distclean: clean
	rm -f $(TARGET).vvp

pull:
	git pull

# End of file (Makefile)

